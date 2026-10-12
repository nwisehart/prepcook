//! Wall-clock scheduling for the once-a-day posts.
//!
//! Every other sweep in the orchestrator is elapsed-interval — a `LAST_SWEEP`
//! instant plus `elapsed()` — which cannot express "08:00 Pacific". The runner
//! is UTC and Pacific shifts by an hour twice a year, so due-ness is decided
//! against a named timezone rather than a stored offset.
//!
//! The date already sent is persisted, not the instant: a restart mid-morning
//! must neither post a second time nor wait until tomorrow.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::{LazyLock, Mutex},
};

use chrono::{DateTime, Datelike, NaiveDate, NaiveTime, Utc, Weekday};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use tracing::warn;
use uuid::Uuid;

/// Timezone used when a time is configured but no zone is.
pub const DEFAULT_TIMEZONE: &str = "America/Los_Angeles";

/// Whether a daily post is due now.
///
/// Due means the local clock has passed `at` and nothing has gone out for that
/// local date. A post the process was down for still goes out later the same
/// day, which is the behaviour worth having: a digest at 09:30 after a restart
/// is useful, and a silent skip looks identical to nothing being wrong.
pub fn is_due(now: DateTime<Utc>, tz: Tz, at: NaiveTime, last_sent: Option<NaiveDate>) -> bool {
    let local = now.with_timezone(&tz);
    // Both daily posts are written for people at work: a Saturday to-do list is
    // read by nobody, and a status update reaches a reporter who cannot act on
    // it until Monday. A Friday slot missed by an outage therefore stays missed
    // rather than arriving stale over the weekend.
    if matches!(local.weekday(), Weekday::Sat | Weekday::Sun) {
        return false;
    }
    local.time() >= at && last_sent != Some(local.date_naive())
}

/// The local date a post going out now belongs to.
pub fn local_date(now: DateTime<Utc>, tz: Tz) -> NaiveDate {
    now.with_timezone(&tz).date_naive()
}

/// Business days from `from` (exclusive) to `to` (inclusive).
///
/// Business days because everything else here skips weekends: a ticket that
/// has not moved since Friday has not moved for one day by Monday, and telling
/// a reporter it has been three is both wrong and the kind of wrong that
/// invites them to stop believing the number.
pub fn business_days_between(from: NaiveDate, to: NaiveDate) -> u32 {
    // A corrupt or hand-edited date must not spin this for ever.
    const MAX_DAYS: u32 = 400;
    let mut days = 0;
    let mut cursor = from;
    while cursor < to && days < MAX_DAYS {
        cursor = match cursor.succ_opt() {
            Some(next) => next,
            None => break,
        };
        if !matches!(cursor.weekday(), Weekday::Sat | Weekday::Sun) {
            days += 1;
        }
    }
    days
}

/// The next date a daily post will go out after `date`.
///
/// Printed on the canvas so a page that has not moved since Friday reads as
/// scheduled rather than broken — the weekday skip is invisible otherwise, and
/// a stale-looking status page is worse than no status page.
pub fn next_run_date(date: NaiveDate) -> NaiveDate {
    let mut next = date.succ_opt().unwrap_or(date);
    while matches!(next.weekday(), Weekday::Sat | Weekday::Sun) {
        next = match next.succ_opt() {
            Some(day) => day,
            None => return next,
        };
    }
    next
}

/// Parses a configured `HH:MM`. Anything else reads as unset, so a typo
/// silences the post rather than firing it at midnight.
pub fn parse_time(raw: &str) -> Option<NaiveTime> {
    NaiveTime::parse_from_str(raw.trim(), "%H:%M").ok()
}

/// Parses an IANA timezone name, falling back to [`DEFAULT_TIMEZONE`].
pub fn parse_timezone(raw: Option<&str>) -> Tz {
    raw.map(str::trim)
        .filter(|name| !name.is_empty())
        .and_then(|name| name.parse::<Tz>().ok())
        .unwrap_or_else(|| {
            DEFAULT_TIMEZONE
                .parse::<Tz>()
                .expect("DEFAULT_TIMEZONE is a valid IANA name")
        })
}

/// Last local date each board's daily posts went out, keyed by project id,
/// plus the last update posted on each ticket.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct DailyState {
    #[serde(default)]
    digest: BTreeMap<String, NaiveDate>,
    #[serde(default)]
    eob: BTreeMap<String, NaiveDate>,
    /// Newest `EOB update:` posted per issue. Held as the text rather than a
    /// hash so a stuck ticket can be diagnosed by reading this file, and
    /// because it is what the repeat check compares against.
    #[serde(default)]
    eob_last: BTreeMap<String, String>,
    /// The date each issue's current `eob_last` text first appeared.
    ///
    /// Not when it was last posted — when it last *changed*. That is the
    /// number a reporter is actually asking for when they ask whether anyone
    /// is looking at their ticket.
    #[serde(default)]
    eob_since: BTreeMap<String, NaiveDate>,
    /// Boards whose one-time merge-request link backfill has completed.
    #[serde(default)]
    mr_links_backfilled: BTreeSet<String>,
    /// Boards whose one-time merge-request review backfill has completed.
    #[serde(default)]
    mr_reviews_backfilled: BTreeSet<String>,
    /// Channel-canvas id per board. Cached because a channel may hold only one
    /// canvas: without the id, every run would have to ask Slack whether it
    /// already made one.
    #[serde(default)]
    canvas: BTreeMap<String, String>,
    /// Jira key → summary for tickets the watcher found assigned to the agent
    /// with a finished card and a still-working Jira status. Written by the
    /// watcher, drained by the morning digest.
    #[serde(default)]
    stuck_hints: BTreeMap<String, String>,
}

impl DailyState {
    pub fn last_digest(&self, project_id: Uuid) -> Option<NaiveDate> {
        self.digest.get(&project_id.to_string()).copied()
    }

    pub fn record_digest(&mut self, project_id: Uuid, date: NaiveDate) {
        self.digest.insert(project_id.to_string(), date);
    }

    pub fn canvas_id(&self, project_id: Uuid) -> Option<&str> {
        self.canvas.get(&project_id.to_string()).map(String::as_str)
    }

    pub fn record_canvas_id(&mut self, project_id: Uuid, canvas_id: &str) {
        self.canvas
            .insert(project_id.to_string(), canvas_id.to_string());
    }

    pub fn last_eob(&self, project_id: Uuid) -> Option<NaiveDate> {
        self.eob.get(&project_id.to_string()).copied()
    }

    pub fn record_eob(&mut self, project_id: Uuid, date: NaiveDate) {
        self.eob.insert(project_id.to_string(), date);
    }

    /// Whether this board's one-time merge-request link backfill has run.
    pub fn mr_links_backfilled(&self, project_id: Uuid) -> bool {
        self.mr_links_backfilled.contains(&project_id.to_string())
    }

    pub fn record_mr_links_backfilled(&mut self, project_id: Uuid) {
        self.mr_links_backfilled.insert(project_id.to_string());
    }

    /// Whether this board's one-time review-comment backfill has run.
    pub fn mr_reviews_backfilled(&self, project_id: Uuid) -> bool {
        self.mr_reviews_backfilled.contains(&project_id.to_string())
    }

    pub fn record_mr_reviews_backfilled(&mut self, project_id: Uuid) {
        self.mr_reviews_backfilled.insert(project_id.to_string());
    }

    pub fn last_update(&self, issue_id: Uuid) -> Option<&str> {
        self.eob_last.get(&issue_id.to_string()).map(String::as_str)
    }

    /// The date this issue's update last changed, if one is recorded.
    pub fn unchanged_since(&self, issue_id: Uuid) -> Option<NaiveDate> {
        self.eob_since.get(&issue_id.to_string()).copied()
    }

    pub fn record_update(&mut self, issue_id: Uuid, update: &str, on: NaiveDate) {
        let key = issue_id.to_string();
        // Only a *different* answer restarts the clock. An entry with no
        // date yet gets one too: texts recorded before `eob_since` existed
        // otherwise never had a start, and the digest could never call them
        // stalled.
        if self.eob_last.get(&key).map(String::as_str) != Some(update)
            || !self.eob_since.contains_key(&key)
        {
            self.eob_since.insert(key.clone(), on);
        }
        self.eob_last.insert(key, update.to_string());
    }

    pub fn record_stuck_hint(&mut self, jira_key: &str, summary: &str) {
        self.stuck_hints
            .insert(jira_key.to_string(), summary.to_string());
    }

    /// Hands over every recorded hint and forgets them, so one stuck ticket is
    /// reported on one morning rather than every morning until someone acts.
    pub fn take_stuck_hints(&mut self) -> BTreeMap<String, String> {
        std::mem::take(&mut self.stuck_hints)
    }
}

/// The one in-memory copy of the state file.
///
/// Both the watcher and the orchestrator write it, so neither may hold its own:
/// two owners would each save a copy missing the other's changes.
static STATE: LazyLock<Mutex<DailyState>> = LazyLock::new(|| Mutex::new(load_state()));

/// Reads the shared state.
pub fn read<T>(f: impl FnOnce(&DailyState) -> T) -> T {
    f(&STATE.lock().unwrap())
}

/// Mutates the shared state and persists it.
pub fn update<T>(f: impl FnOnce(&mut DailyState) -> T) -> T {
    let mut state = STATE.lock().unwrap();
    let out = f(&mut state);
    save_state(&state);
    out
}

fn state_path() -> PathBuf {
    utils::assets::asset_dir().join("daily_state.json")
}

pub fn load_state() -> DailyState {
    std::fs::read_to_string(state_path())
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub fn save_state(state: &DailyState) {
    if let Ok(raw) = serde_json::to_string_pretty(state)
        && let Err(error) = std::fs::write(state_path(), raw)
    {
        warn!(?error, "daily: failed to persist state");
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn pacific() -> Tz {
        parse_timezone(None)
    }

    fn at(hour: u32, minute: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(hour, minute, 0).unwrap()
    }

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn business_days_do_not_count_the_weekend() {
        let day = |d| NaiveDate::from_ymd_opt(2026, 9, d).unwrap();
        // 2026-09-14 is a Monday.
        assert_eq!(business_days_between(day(14), day(14)), 0);
        assert_eq!(business_days_between(day(14), day(15)), 1);
        assert_eq!(business_days_between(day(14), day(18)), 4);
        // Friday to Monday is one working day, not three. Reporting three is
        // what makes a reporter stop trusting the number.
        assert_eq!(business_days_between(day(18), day(21)), 1);
        // The week Emelie asked four times: 15 Sep to 21 Sep.
        assert_eq!(business_days_between(day(15), day(21)), 4);
        // A date in the future cannot produce a negative or wrapped count.
        assert_eq!(business_days_between(day(21), day(14)), 0);
    }

    #[test]
    fn the_next_run_skips_the_weekend() {
        // 2026-09-04 is a Friday.
        assert_eq!(next_run_date(day(2026, 9, 4)), day(2026, 9, 7));
        assert_eq!(next_run_date(day(2026, 9, 7)), day(2026, 9, 8));
        // A Saturday date never posts, but the canvas may still carry one from
        // a Friday run: the answer is Monday either way.
        assert_eq!(next_run_date(day(2026, 9, 5)), day(2026, 9, 7));
    }

    #[test]
    fn eight_pacific_is_sixteen_utc_in_winter_and_fifteen_in_summer() {
        // The whole reason for a named zone: one stored offset is wrong for
        // eight months of the year.
        let winter = Utc.with_ymd_and_hms(2026, 1, 15, 16, 0, 0).unwrap();
        let summer = Utc.with_ymd_and_hms(2026, 7, 15, 15, 0, 0).unwrap();
        assert!(is_due(winter, pacific(), at(8, 0), None));
        assert!(is_due(summer, pacific(), at(8, 0), None));

        let winter_early = Utc.with_ymd_and_hms(2026, 1, 15, 15, 0, 0).unwrap();
        assert!(!is_due(winter_early, pacific(), at(8, 0), None));
    }

    #[test]
    fn a_post_already_sent_today_does_not_repeat() {
        let now = Utc.with_ymd_and_hms(2026, 7, 15, 17, 0, 0).unwrap();
        let today = local_date(now, pacific());
        assert!(!is_due(now, pacific(), at(8, 0), Some(today)));
        assert!(is_due(
            now,
            pacific(),
            at(8, 0),
            Some(today.pred_opt().unwrap())
        ));
    }

    #[test]
    fn a_post_missed_while_the_process_was_down_still_goes_out() {
        // 11:00 Pacific, nothing sent today: the 08:00 slot was missed, and
        // late is worth more than silent.
        let now = Utc.with_ymd_and_hms(2026, 7, 15, 18, 0, 0).unwrap();
        assert!(is_due(now, pacific(), at(8, 0), None));
    }

    #[test]
    fn the_weekend_is_silent() {
        // 10:00 Pacific on each, well past an 08:00 slot, nothing sent.
        let saturday = Utc.with_ymd_and_hms(2026, 7, 18, 17, 0, 0).unwrap();
        let sunday = Utc.with_ymd_and_hms(2026, 7, 19, 17, 0, 0).unwrap();
        let monday = Utc.with_ymd_and_hms(2026, 7, 20, 17, 0, 0).unwrap();
        assert!(!is_due(saturday, pacific(), at(8, 0), None));
        assert!(!is_due(sunday, pacific(), at(8, 0), None));
        assert!(is_due(monday, pacific(), at(8, 0), None));
    }

    #[test]
    fn an_unparseable_time_is_no_time_at_all() {
        assert!(parse_time("8am").is_none());
        assert!(parse_time("").is_none());
        assert_eq!(parse_time(" 08:00 "), Some(at(8, 0)));
    }

    #[test]
    fn an_unknown_timezone_falls_back_rather_than_failing() {
        assert_eq!(parse_timezone(Some("Mars/Olympus")), pacific());
        assert_eq!(parse_timezone(None), pacific());
        assert_eq!(
            parse_timezone(Some("Europe/London")),
            "Europe/London".parse::<Tz>().unwrap()
        );
    }

    #[test]
    fn an_unchanged_update_keeps_its_start_date_and_an_undated_one_gets_one() {
        let issue = Uuid::new_v4();
        let mut state = DailyState::default();
        state.record_update(issue, "with engineering", day(2026, 9, 22));
        state.record_update(issue, "with engineering", day(2026, 9, 25));
        assert_eq!(state.unchanged_since(issue), Some(day(2026, 9, 22)));
        state.record_update(issue, "merged", day(2026, 9, 28));
        assert_eq!(state.unchanged_since(issue), Some(day(2026, 9, 28)));

        // Recorded before `eob_since` existed: the text is there, its date is not.
        let legacy = Uuid::new_v4();
        state
            .eob_last
            .insert(legacy.to_string(), "with engineering".to_string());
        state.record_update(legacy, "with engineering", day(2026, 10, 1));
        assert_eq!(state.unchanged_since(legacy), Some(day(2026, 10, 1)));
    }
}

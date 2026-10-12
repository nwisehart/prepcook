//! Subscription-seat usage-limit backoff.
//!
//! When the Claude seat hits a usage window, every launch fails until that
//! window resets — and an orchestrator that keeps launching just burns the
//! window the moment it reopens. So a refusal pauses launches, and the pause
//! runs until the window the seat named actually reopens.
//!
//! Two channels report on the seat, and neither is complete on its own:
//!
//! - [`scan_text`] sees the refusal, as a turn payload
//!   ("You've hit your session limit · resets 2:40am (UTC)"). It is a substring
//!   match on text an agent can also produce, and it names the reset only in
//!   prose.
//! - [`record_rate_limit_event`] sees the machine-readable state — status,
//!   window, reset instant — but is emitted when that state *changes*, so it
//!   cannot be waited on to discover a refusal.
//!
//! **A refusal is only about now if the window it names is still open.** The
//! same refusal text reaches this module long after the fact: resuming a
//! session replays its transcript, and the synthetic message carrying an old
//! refusal parses exactly like a live one. On the runner that re-paused the
//! whole board 90 minutes after the window had reopened, every time a follow-up
//! resumed the session — and a stage that retries its follow-up re-armed it
//! every tick, indefinitely. Every genuine rejection observed named a reset in
//! its own future; every stale one named a reset in the past. So a refusal
//! whose window has closed is dropped rather than trusted: if it is history,
//! pausing is pure loss, and if the seat really is refusing on a dead window,
//! finding out costs one failed launch.
//!
//! The stage orchestrator checks [`pause_remaining`] before launching,
//! [`pause_detail`] to say why the board is idle, and [`take_notify`] /
//! [`take_exhausted_notify`] to alert Slack.
//!
//! Process-wide state (like the seat monitor's degraded flag): written by the
//! executor's log parser, read by services. A restart clears the pause, which
//! at worst costs one failed launch to re-arm it.

use std::{
    sync::{Mutex, MutexGuard},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use chrono::{DateTime, NaiveTime, TimeZone, Utc};

/// Pause length when a refusal arrives and nothing has told us when the window
/// reopens. Coarse on purpose: it exists to stop the hammering, and the next
/// rate-limit event replaces it with the real instant.
const FALLBACK_PAUSE: Duration = Duration::from_secs(30 * 60);

/// Ceiling on a pause derived from a reported reset instant.
///
/// A seven-day window can reset days out, and a board frozen that long on the
/// strength of one parsed field is worse than the alternative: waking early
/// costs one refused launch, which re-arms the pause with a fresh instant.
const MAX_PAUSE: Duration = Duration::from_secs(12 * 60 * 60);

/// Re-arms with nothing served in between before the seat is reported as unable
/// to carry the board. Each pause can be legitimate and the board still be idle
/// all day, which is worth saying out loud once.
const EXHAUSTED_REARM_THRESHOLD: u32 = 3;

/// Minimum gap between seat-limit Slack notifications.
///
/// One notification per *pause* is not enough on its own: a seat that stays
/// exhausted re-arms every time a launch is refused, and each re-arm is a
/// legitimately fresh pause. That produced four identical messages in ninety
/// minutes on 2026-08-13, none of which said anything the first had not. The log
/// line still fires every time.
const NOTIFY_COOLDOWN: Duration = Duration::from_secs(2 * 60 * 60);

/// Lowercased markers seen in Claude Code output when the subscription seat is
/// out of budget (distinct from auth failures, which the seat monitor
/// classifies separately).
const LIMIT_MARKERS: [&str; 4] = [
    "hit your session limit",
    "session limit reached",
    "hit your usage limit",
    "usage limit reached",
];

/// Longest error payload still read as a seat-limit report.
///
/// A real report from the CLI is short and *is* the whole payload
/// ("You've hit your session limit · resets 2:40am (UTC)"). The same words
/// inside kilobytes of agent output mean an agent was talking about limits —
/// quoting a log, summarising an RCA, or reading this very file, which contains
/// every marker above. Pausing on that freezes every board while the seat is
/// perfectly healthy, which is what happened twice on 2026-08-17.
const MAX_LIMIT_PAYLOAD_CHARS: usize = 300;

/// Widest gap between now and a bare clock time in a refusal caption before the
/// reading is discarded as not-this-window. Captions name a boundary a few
/// hours either side of now; anything further means the hour was misread.
const MAX_CAPTION_SKEW_HOURS: i64 = 12;

/// What a scanned payload means.
enum Verdict {
    /// No marker anywhere.
    NoMarker,
    /// Marker present, but the payload is too long to be the CLI reporting a
    /// limit. Kept distinct from `NoMarker` so the near miss can be logged:
    /// silently ignoring it would hide a marker the CLI really did start
    /// emitting in long payloads.
    MarkerInLongPayload { marker: &'static str, chars: usize },
    /// A refusal. `caption_reset_ms` is the instant its caption named, read
    /// only closely enough to tell whether that window has already closed.
    Limit {
        marker: &'static str,
        caption_reset_ms: Option<u64>,
    },
}

/// Reads the reset instant out of a refusal caption, e.g. the "2:40am (UTC)" in
/// "You've hit your session limit · resets 2:40am (UTC)".
///
/// Only an explicitly-UTC caption is read. The same message rendered in a local
/// zone would parse to a different instant, and the runner logs UTC, so the
/// narrow reading loses nothing there and stays silent elsewhere.
///
/// The result is trusted in one direction only, and [`scan_text`] enforces it:
/// a caption resolving *behind* now proves the refusal is history, while one
/// resolving ahead proves nothing. The caption carries no date, so a clock time
/// is resolved to the nearest candidate day — which for "2:40am" read at 03:50
/// is this morning, an hour behind, but read at 16:00 is tomorrow morning, eight
/// hours ahead. Sizing a pause from the second reading would idle the board all
/// night on a replayed message.
fn reset_from_caption(lowered: &str) -> Option<u64> {
    if !lowered.contains("(utc)") {
        return None;
    }
    let after = lowered.split("resets ").nth(1)?;
    let time = parse_clock(after)?;

    let now = Utc::now();
    // Yesterday, today, tomorrow: a boundary near midnight is named by a clock
    // time whose nearest instant falls on the adjacent day.
    (-1..=1)
        .map(|offset| {
            let day = now.date_naive() + chrono::Duration::days(offset);
            Utc.from_utc_datetime(&day.and_time(time))
        })
        .min_by_key(|candidate: &DateTime<Utc>| (*candidate - now).num_seconds().abs())
        .filter(|candidate| (*candidate - now).num_hours().abs() <= MAX_CAPTION_SKEW_HOURS)
        .map(|candidate| candidate.timestamp_millis().max(0) as u64)
}

/// Parses a leading `H:MM`, optionally followed by `am`/`pm`, from a lowercased
/// caption tail.
fn parse_clock(tail: &str) -> Option<NaiveTime> {
    let digits_and_colon: String = tail
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == ':')
        .collect();
    let (hour, minute) = digits_and_colon.split_once(':')?;
    let mut hour: u32 = hour.parse().ok()?;
    let minute: u32 = minute.parse().ok()?;

    let suffix = tail[digits_and_colon.len()..].trim_start();
    if suffix.starts_with("pm") && hour < 12 {
        hour += 12;
    } else if suffix.starts_with("am") && hour == 12 {
        // 12:xxam is the small hours, not noon.
        hour = 0;
    }
    NaiveTime::from_hms_opt(hour, minute, 0)
}

/// Classifies an error payload. Pure, so the rules are testable without
/// touching the shared pause state.
fn classify(text: &str) -> Verdict {
    let lowered = text.to_ascii_lowercase();
    let Some(marker) = LIMIT_MARKERS
        .iter()
        .copied()
        .find(|marker| lowered.contains(marker))
    else {
        return Verdict::NoMarker;
    };
    // chars(), not len(): a payload of multi-byte characters must not read as
    // longer than it is.
    let chars = lowered.chars().count();
    if chars > MAX_LIMIT_PAYLOAD_CHARS {
        return Verdict::MarkerInLongPayload { marker, chars };
    }
    Verdict::Limit {
        marker,
        caption_reset_ms: reset_from_caption(&lowered),
    }
}

/// Which signal armed a pause. Determines whether a served turn may override
/// it: a text match is a guess about a payload the agent can influence, a
/// rate-limit event is the seat's own report.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PauseSource {
    RateLimitEvent,
    ErrorText,
}

/// Why launches are paused, for logs, Slack and the board banner.
#[derive(Clone, Debug)]
pub struct PauseDetail {
    pub remaining: Duration,
    pub source: PauseSource,
    /// `rateLimitType` from the seat's report (`five_hour`, `seven_day_opus`,
    /// …). None until a rate-limit event has named one: the refusal text never
    /// does.
    pub limit_type: Option<String>,
    /// The window has been exhausted [`EXHAUSTED_REARM_THRESHOLD`] times over
    /// with nothing served in between, so the board is not merely waiting out
    /// one window.
    pub repeatedly_exhausted: bool,
    /// Consecutive arms with nothing served in between.
    pub consecutive_arms: u32,
    /// Shortened payload that armed the pause.
    pub report: Option<String>,
}

/// Outcome of feeding a refusal to the state.
enum ArmOutcome {
    /// Launches are paused.
    Armed(PauseDetail),
    /// The refusal names a window that closed `behind` ago, so it describes the
    /// past. Launches keep running.
    Stale { behind: Duration, total: u32 },
}

struct State {
    /// Epoch ms until which launches are paused. None = launches running.
    pause_until_ms: Option<u64>,
    source: Option<PauseSource>,
    limit_type: Option<String>,
    consecutive_arms: u32,
    repeatedly_exhausted: bool,
    report: Option<String>,
    /// Most recent reset instant the seat has reported, epoch ms, from any
    /// rate-limit event including the healthy ones. Used to date a refusal
    /// whose own caption could not be read.
    last_known_reset_ms: Option<u64>,
    /// Refusals dropped as history, cumulative. Never reset: the count is the
    /// evidence that replayed refusals are still arriving.
    stale_refusals: u32,
    notify_pending: bool,
    exhausted_notify_pending: bool,
    last_notify_ms: u64,
}

static STATE: Mutex<State> = Mutex::new(State {
    pause_until_ms: None,
    source: None,
    limit_type: None,
    consecutive_arms: 0,
    repeatedly_exhausted: false,
    report: None,
    last_known_reset_ms: None,
    stale_refusals: 0,
    notify_pending: false,
    exhausted_notify_pending: false,
    last_notify_ms: 0,
});

/// A poisoned lock means a caller panicked mid-update, not that the data is
/// unusable: recovering beats propagating a panic into every parsed turn and
/// every `/api/info` request.
fn state() -> MutexGuard<'static, State> {
    STATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn now_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

/// Shortens a payload for operator-facing output.
fn summarize(text: &str) -> String {
    text.chars().take(200).collect()
}

impl State {
    /// Feeds a refusal in. `reset_ms` is the instant the refusal itself named,
    /// if any; the last window the seat reported stands in when it named none.
    fn arm(
        &mut self,
        source: PauseSource,
        reset_ms: Option<u64>,
        limit_type: Option<String>,
        report: Option<String>,
    ) -> ArmOutcome {
        let now = now_epoch_ms();
        let reset_ms = reset_ms.or(self.last_known_reset_ms);

        if let Some(reset) = reset_ms
            && reset <= now
        {
            self.stale_refusals = self.stale_refusals.saturating_add(1);
            return ArmOutcome::Stale {
                behind: Duration::from_millis(now - reset),
                total: self.stale_refusals,
            };
        }

        let was_paused = self.pause_until_ms.is_some_and(|until| until > now);
        let pause_for = match reset_ms {
            Some(reset) => Duration::from_millis(reset - now).min(MAX_PAUSE),
            None => FALLBACK_PAUSE,
        };

        self.pause_until_ms = Some(now + pause_for.as_millis() as u64);
        self.source = Some(source);
        if limit_type.is_some() {
            self.limit_type = limit_type;
        }
        if report.is_some() {
            self.report = report;
        }
        self.consecutive_arms = self.consecutive_arms.saturating_add(1);

        let exhausted = self.consecutive_arms >= EXHAUSTED_REARM_THRESHOLD;
        // Alert on entering the state, not on every re-arm inside it.
        if exhausted && !self.repeatedly_exhausted {
            self.exhausted_notify_pending = true;
        }
        self.repeatedly_exhausted = exhausted;

        if !was_paused
            && (self.last_notify_ms == 0
                || now.saturating_sub(self.last_notify_ms) > NOTIFY_COOLDOWN.as_millis() as u64)
        {
            self.last_notify_ms = now;
            self.notify_pending = true;
        }

        ArmOutcome::Armed(PauseDetail {
            remaining: pause_for,
            source,
            limit_type: self.limit_type.clone(),
            repeatedly_exhausted: self.repeatedly_exhausted,
            consecutive_arms: self.consecutive_arms,
            report: self.report.clone(),
        })
    }

    /// Ends the pause and forgets what caused it. Leaves `last_known_reset_ms`
    /// and `stale_refusals` alone: those are facts about the seat, not about
    /// this pause.
    fn disarm(&mut self) -> bool {
        let was_paused = self
            .pause_until_ms
            .is_some_and(|until| until > now_epoch_ms());
        self.pause_until_ms = None;
        self.source = None;
        self.limit_type = None;
        self.consecutive_arms = 0;
        self.repeatedly_exhausted = false;
        self.report = None;
        // Drop any queued alert too: announcing a pause that has just been
        // lifted is worse than saying nothing.
        self.notify_pending = false;
        self.exhausted_notify_pending = false;
        was_paused
    }
}

/// Logs the outcome of a refusal. Kept out of [`State::arm`] so the lock is
/// never held across a tracing call.
fn report_outcome(outcome: ArmOutcome, context: &str) {
    match outcome {
        ArmOutcome::Armed(detail) => tracing::warn!(
            limit_type = detail.limit_type.as_deref().unwrap_or("unknown"),
            consecutive_arms = detail.consecutive_arms,
            repeatedly_exhausted = detail.repeatedly_exhausted,
            "Claude seat limit hit — pausing agent launches for {}m ({context})",
            detail.remaining.as_secs() / 60
        ),
        ArmOutcome::Stale { behind, total } => tracing::warn!(
            stale_refusals = total,
            "ignoring a seat refusal for a window that reopened {}m ago — \
             replayed history, not a live limit ({context})",
            behind.as_secs() / 60
        ),
    }
}

/// Reads a reported reset instant as epoch milliseconds.
///
/// The field is a bare number and the seat is the only thing that knows which
/// unit it means, so the magnitude decides: anything below this threshold is too
/// small to be a plausible millisecond timestamp (it would land in 1973) and is
/// read as seconds. Guessing wrong would be a thousandfold error in the pause
/// length, so the unit is inferred rather than assumed. Runner payloads observed
/// 2026-08-20 were seconds (`1787254800` = 19:40 UTC that day).
const MAX_PLAUSIBLE_EPOCH_SECONDS: f64 = 100_000_000_000.0;

fn reset_instant_ms(info: &serde_json::Value) -> Option<u64> {
    let value = info
        .get("resetsAt")
        .and_then(|value| value.as_f64())
        .filter(|number| *number > 0.0)?;
    Some(if value < MAX_PLAUSIBLE_EPOCH_SECONDS {
        (value * 1000.0) as u64
    } else {
        value as u64
    })
}

/// Records a `rate_limit_event` from Claude Code — the seat's own account of its
/// windows, so it both arms and clears: `status: "rejected"` pauses until the
/// reported reset, anything else means requests are being served and ends any
/// pause in force.
///
/// Shape (`SDKRateLimitInfo`): `status` is `allowed` | `allowed_warning` |
/// `rejected`; `resetsAt` is an epoch instant; `rateLimitType` names the window
/// (`five_hour`, `seven_day`, `seven_day_opus`, …). Missing fields degrade to
/// the fallback pause rather than being read as healthy.
pub fn record_rate_limit_event(info: &serde_json::Value) {
    let status = info.get("status").and_then(|value| value.as_str());
    let limit_type = info
        .get("rateLimitType")
        .and_then(|value| value.as_str())
        .map(str::to_string);

    // Recorded from healthy events too: this is the only place the reset instant
    // is machine-readable, and a refusal that cannot date itself needs it.
    if let Some(reset_ms) = reset_instant_ms(info) {
        state().last_known_reset_ms = Some(reset_ms);
    }

    // Overage carries the seat past an exhausted window, so a rejection the
    // overage budget is actively absorbing is no reason to stop launching. Only
    // `isUsingOverage` says that; an `overageStatus` of "allowed" means overage
    // *could* apply, which is not the same as it applying.
    let overage_covering = info
        .get("isUsingOverage")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);

    match status {
        Some("rejected") if !overage_covering => {
            let outcome = state().arm(
                PauseSource::RateLimitEvent,
                reset_instant_ms(info),
                limit_type,
                Some(summarize(&info.to_string())),
            );
            report_outcome(outcome, "reported by the seat");
        }
        Some(_) => {
            // The seat says it is serving requests. That outranks anything a
            // payload substring suggested, and it is the only prompt signal that
            // a window reopened.
            let was_paused = state().disarm();
            if was_paused {
                tracing::info!(
                    "Claude seat reports it is serving requests again — launches resume"
                );
            }
        }
        None => {}
    }
}

/// Scans a turn's error output for a refusal. Safe to call on every turn error.
///
/// The weaker of the two signals: it cannot name the exhausted window, it
/// matches text an agent can produce, and it arrives again whenever a session
/// replays. So the caption's own reset instant decides whether it is current,
/// and [`note_served_turn`] can overrule it.
///
/// Returns whether this payload stopped the turn: true only when it armed the
/// pause. A replayed refusal reads as history and returns false, because the
/// session it appears in is not waiting on anything.
pub fn scan_text(text: &str) -> bool {
    let (marker, caption_reset_ms) = match classify(text) {
        Verdict::NoMarker => return false,
        Verdict::MarkerInLongPayload { marker, chars } => {
            tracing::debug!(
                "seat-limit marker {marker:?} inside a {chars}-char payload — \
                 read as agent output, not a limit report"
            );
            return false;
        }
        Verdict::Limit {
            marker,
            caption_reset_ms,
        } => (marker, caption_reset_ms),
    };
    // Log the matched marker and the payload: "something matched" is not enough
    // to tell a real limit from a false positive after the fact.
    let context = format!("matched {marker:?} in {:?}", summarize(text));

    // The caption vetoes, it does not schedule. A window it puts behind us is
    // closed whichever day the clock time belongs to; a window it puts ahead
    // could equally be tomorrow's boundary on a replayed message, so the pause
    // is sized from the seat's reported instant instead.
    if let Some(closed) = caption_reset_ms.filter(|reset| *reset <= now_epoch_ms()) {
        let behind = Duration::from_millis(now_epoch_ms().saturating_sub(closed));
        let total = {
            let mut state = state();
            state.stale_refusals = state.stale_refusals.saturating_add(1);
            state.stale_refusals
        };
        report_outcome(ArmOutcome::Stale { behind, total }, &context);
        return false;
    }

    let outcome = state().arm(PauseSource::ErrorText, None, None, Some(summarize(text)));
    let armed = matches!(outcome, ArmOutcome::Armed(_));
    report_outcome(outcome, &context);
    armed
}

/// Records that a turn completed without error, which proves the seat is
/// serving requests.
///
/// This ends a pause armed from matched text — the guess is disproven — but
/// leaves a rate-limit-event pause standing: a turn already in flight when the
/// window closed finishes fine, and its success says nothing about starting a
/// new one.
pub fn note_served_turn() {
    let disproven = {
        let mut state = state();
        state.source == Some(PauseSource::ErrorText) && state.disarm()
    };
    if disproven {
        tracing::info!(
            "a Claude turn completed while launches were paused on a matched \
             limit message — the pause was a false reading, launches resume"
        );
    }
}

/// Remaining pause, if launches are currently paused.
pub fn pause_remaining() -> Option<Duration> {
    let until = state().pause_until_ms?;
    let now = now_epoch_ms();
    (until > now).then(|| Duration::from_millis(until - now))
}

/// Remaining pause with the context needed to explain it.
pub fn pause_detail() -> Option<PauseDetail> {
    let state = state();
    let until = state.pause_until_ms?;
    let now = now_epoch_ms();
    (until > now).then(|| PauseDetail {
        remaining: Duration::from_millis(until - now),
        source: state.source.unwrap_or(PauseSource::ErrorText),
        limit_type: state.limit_type.clone(),
        repeatedly_exhausted: state.repeatedly_exhausted,
        consecutive_arms: state.consecutive_arms,
        report: state.report.clone(),
    })
}

/// Refusals dropped as replayed history since startup. Zero is the healthy
/// value; a climbing count means sessions are still replaying old refusals.
pub fn stale_refusals_seen() -> u32 {
    state().stale_refusals
}

/// Consumes the pending one-shot notification (true at most once per pause, and
/// at most once per [`NOTIFY_COOLDOWN`]).
pub fn take_notify() -> bool {
    std::mem::take(&mut state().notify_pending)
}

/// Consumes the one-shot alert for a seat that keeps running out. Separate from
/// [`take_notify`] because the remedy differs: one pause clears itself, a board
/// that never gets a turn through needs less burn or more budget.
pub fn take_exhausted_notify() -> bool {
    std::mem::take(&mut state().exhausted_notify_pending)
}

/// Ends the pause now. Returns whether one was actually in force.
///
/// The operator override for a misfire. Callers should confirm the seat is
/// healthy first (see the seat probe); clearing a pause that reflects a real
/// limit just spends one launch re-arming it.
pub fn clear_pause() -> bool {
    let was_paused = state().disarm();
    if was_paused {
        tracing::info!("Claude seat pause cleared by request — launches resume");
    }
    was_paused
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Returns the shared state to its startup values. `disarm` deliberately
    /// keeps the last reported window and the stale count — facts about the
    /// seat, not about one pause — so tests need a fuller reset than production
    /// code ever wants.
    /// The pause state is process-wide, so the stateful tests take this before
    /// touching it — `cargo test` runs them on separate threads and they would
    /// otherwise clobber each other's assertions.
    static TEST_GUARD: Mutex<()> = Mutex::new(());

    fn serialised() -> MutexGuard<'static, ()> {
        let guard = TEST_GUARD
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        reset_shared_state();
        guard
    }

    fn reset_shared_state() {
        let mut state = state();
        state.disarm();
        state.last_known_reset_ms = None;
        state.stale_refusals = 0;
        state.last_notify_ms = 0;
    }

    fn epoch_seconds_from_now(offset: i64) -> i64 {
        (now_epoch_ms() / 1000) as i64 + offset
    }

    #[test]
    fn pause_lifecycle() {
        let _serialised = serialised();

        scan_text("irrelevant error");
        assert!(pause_remaining().is_none());
        assert!(!take_notify());

        // A refusal with no readable caption and no reported window: paused for
        // the fallback interval, marked as the weaker source.
        scan_text("session limit reached");
        let detail = pause_detail().expect("a refusal arms the pause");
        assert_eq!(detail.source, PauseSource::ErrorText);
        assert_eq!(detail.remaining, FALLBACK_PAUSE);
        assert!(detail.limit_type.is_none());
        assert!(take_notify());
        assert!(!take_notify());

        // A served turn disproves a text-derived pause.
        note_served_turn();
        assert!(pause_remaining().is_none());

        // A rejection with a reset instant pauses until that instant and names
        // the window.
        record_rate_limit_event(&serde_json::json!({
            "status": "rejected",
            "resetsAt": epoch_seconds_from_now(3600),
            "rateLimitType": "seven_day_opus",
        }));
        let detail = pause_detail().expect("a rejection arms the pause");
        assert_eq!(detail.source, PauseSource::RateLimitEvent);
        assert_eq!(detail.limit_type.as_deref(), Some("seven_day_opus"));
        assert!(detail.remaining > Duration::from_secs(59 * 60));

        // A served turn must not override the seat's own rejection.
        note_served_turn();
        assert!(pause_remaining().is_some());

        // The seat reporting it is serving requests does.
        record_rate_limit_event(&serde_json::json!({ "status": "allowed" }));
        assert!(pause_remaining().is_none());

        // Operator override.
        scan_text("session limit reached");
        assert!(clear_pause());
        assert!(!clear_pause());
    }

    /// A refusal whose caption names a clock time `hours_from_now` away, so an
    /// assertion about it does not depend on when the suite happens to run.
    fn refusal_caption(hours_from_now: i64) -> String {
        let at = Utc::now() + chrono::Duration::hours(hours_from_now);
        format!(
            "success  \"You've hit your session limit · resets {} (UTC)\"",
            at.format("%H:%M")
        )
    }

    #[test]
    fn a_refusal_for_a_reopened_window_is_ignored() {
        let _serialised = serialised();
        // The runner's shape: a follow-up resumes a session and replays the
        // refusal recorded while the window really was closed.
        record_rate_limit_event(&serde_json::json!({
            "status": "rejected",
            "resetsAt": epoch_seconds_from_now(-3600),
            "rateLimitType": "five_hour",
        }));
        assert!(
            pause_remaining().is_none(),
            "a window that reopened an hour ago must not pause launches"
        );
        assert_eq!(stale_refusals_seen(), 1);

        // And again through the text channel, which is how a replay arrives.
        scan_text(&refusal_caption(-1));
        assert!(pause_remaining().is_none());
        assert_eq!(stale_refusals_seen(), 2);
    }

    #[test]
    fn a_caption_naming_a_future_clock_time_cannot_extend_a_pause() {
        let _serialised = serialised();
        // "resets 2:40am" read in the afternoon resolves to tomorrow morning.
        // Sizing a pause from that reading would idle every board overnight on
        // a replayed message, so a caption is never trusted forward.
        record_rate_limit_event(&serde_json::json!({
            "status": "rejected",
            "resetsAt": epoch_seconds_from_now(-3600),
            "rateLimitType": "five_hour",
        }));
        scan_text(&refusal_caption(8));
        assert!(
            pause_remaining().is_none(),
            "the seat's own reset says the window reopened; a caption eight \
             hours out must not override it"
        );
    }

    #[test]
    fn a_live_refusal_is_sized_by_the_last_reported_window() {
        let _serialised = serialised();
        // A healthy event goes by first, carrying the window's reset instant.
        record_rate_limit_event(&serde_json::json!({
            "status": "allowed",
            "resetsAt": epoch_seconds_from_now(2 * 3600),
            "rateLimitType": "five_hour",
        }));
        assert!(pause_remaining().is_none(), "a healthy event never pauses");

        // The refusal names no instant this parser can read, so the reported
        // window sizes the pause instead of the flat fallback.
        scan_text("usage limit reached");
        let detail = pause_detail().expect("the refusal arms the pause");
        assert!(
            detail.remaining > Duration::from_secs(90 * 60),
            "the pause should run to the reported reset, not the fallback"
        );
    }

    #[test]
    fn repeated_exhaustion_alerts_once() {
        let _serialised = serialised();
        for _ in 0..EXHAUSTED_REARM_THRESHOLD {
            scan_text("session limit reached");
        }
        let detail = pause_detail().expect("a refusal arms the pause");
        assert!(detail.repeatedly_exhausted);
        assert!(take_exhausted_notify());
        assert!(!take_exhausted_notify(), "one alert per episode");
        scan_text("session limit reached");
        assert!(!take_exhausted_notify());
    }

    #[test]
    fn overage_absorbing_a_rejection_does_not_pause() {
        let _serialised = serialised();
        record_rate_limit_event(&serde_json::json!({
            "status": "rejected",
            "resetsAt": epoch_seconds_from_now(3600),
            "isUsingOverage": true,
        }));
        assert!(pause_remaining().is_none());
    }

    // Pure classification and caption parsing, so these never touch the shared
    // state and cannot race the tests above.

    #[test]
    fn short_cli_payloads_are_refusals() {
        for text in [
            "success  \"You've hit your session limit · resets 2:40am (UTC)\"",
            "error_during_execution  Claude AI usage limit reached|1755500000",
            "session limit reached",
            "HIT YOUR USAGE LIMIT",
        ] {
            assert!(
                matches!(classify(text), Verdict::Limit { .. }),
                "expected a refusal: {text}"
            );
        }
    }

    #[test]
    fn a_marker_in_long_agent_output_is_not_a_refusal() {
        // The shape that froze the board on 2026-08-17: an agent's errored turn
        // whose output quoted the limit-detection code itself.
        let prose = format!(
            "error_during_execution  I reviewed crates/executors/src/usage_limits.rs. \
             It matches on \"session limit reached\" and \"hit your usage limit\", so {}",
            "the guardrail is substring-based and can misfire. ".repeat(6)
        );
        assert!(prose.chars().count() > MAX_LIMIT_PAYLOAD_CHARS);
        assert!(matches!(
            classify(&prose),
            Verdict::MarkerInLongPayload { .. }
        ));
    }

    #[test]
    fn unrelated_errors_have_no_marker() {
        assert!(matches!(
            classify("No such file or directory (os error 2)"),
            Verdict::NoMarker
        ));
    }

    #[test]
    fn captions_resolve_to_the_nearest_instant() {
        let now = Utc::now();
        // A caption naming a time an hour out resolves ahead of now, not to the
        // same clock time yesterday.
        let ahead = now + chrono::Duration::hours(1);
        let caption = format!(
            "you've hit your session limit · resets {} (utc)",
            ahead.format("%H:%M")
        );
        let parsed = reset_from_caption(&caption).expect("an explicit UTC caption parses");
        let drift = (parsed as i64 - ahead.timestamp_millis()).abs();
        assert!(
            drift < 60_000,
            "resolved {parsed} should be within a minute"
        );
    }

    #[test]
    fn twelve_hour_captions_parse() {
        assert_eq!(
            parse_clock("2:40am (utc)"),
            NaiveTime::from_hms_opt(2, 40, 0)
        );
        assert_eq!(
            parse_clock("9:40pm (utc)"),
            NaiveTime::from_hms_opt(21, 40, 0)
        );
        assert_eq!(
            parse_clock("12:10am (utc)"),
            NaiveTime::from_hms_opt(0, 10, 0)
        );
        assert_eq!(
            parse_clock("12:10pm (utc)"),
            NaiveTime::from_hms_opt(12, 10, 0)
        );
        assert_eq!(
            parse_clock("14:40 (utc)"),
            NaiveTime::from_hms_opt(14, 40, 0)
        );
    }

    #[test]
    fn a_caption_without_an_explicit_zone_is_not_read() {
        assert!(reset_from_caption("resets 2:40am").is_none());
    }
}

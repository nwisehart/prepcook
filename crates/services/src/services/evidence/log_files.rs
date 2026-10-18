//! Log files the app writes where it runs for the card: globs relative to
//! the agent's worktree (`logs/app.log`), or absolute paths. `search` reads
//! them on this machine; nothing is uploaded anywhere.
//!
//! This is the one kind a repository's own `[[profile.evidence]]` may
//! declare, since its paths are a fact of the repository; the toolkit
//! generator (`toolkit/generator/generate.py`) renders [`describe_paths`] for
//! the manifest it is given, and a test keeps the two identical.

use std::{
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, Utc};
use globset::{GlobBuilder, GlobMatcher};
use serde_json::Value;

use super::{
    EvidenceError, EvidenceKind, EvidenceSource, Hit, McpServerEntry, SearchQuery, SearchResult,
    SourceSettings, capped, with_common_fields,
};
use crate::services::integrations::{Descriptor, Field, FieldKind};

/// Bytes read from the end of each file; older lines of a larger file are
/// not searched.
const READ_CAP: u64 = 32 * 1024 * 1024;
/// Files one pattern may expand to.
const FILE_CAP: usize = 200;
/// How deep a `**` pattern walks.
const WALK_DEPTH: usize = 12;

/// JSON keys a structured log line carries its time under.
const TIME_KEYS: [&str; 7] = [
    "time",
    "timestamp",
    "ts",
    "@timestamp",
    "date",
    "datetime",
    "t",
];

pub struct LogFilesSource {
    settings: SourceSettings,
    patterns: Vec<String>,
    /// Where relative patterns resolve when a search names no worktree.
    root: Option<PathBuf>,
}

impl LogFilesSource {
    pub fn descriptor() -> Descriptor {
        Descriptor::new(
            EvidenceKind::LogFiles.as_str(),
            EvidenceKind::LogFiles.display_name(),
            "Log files the app writes where it runs for the card, read on this machine; a \
             repository can also name its own in [[profile.evidence]].",
            with_common_fields(
                EvidenceKind::LogFiles,
                vec![Field::new(
                    "paths",
                    "Paths",
                    FieldKind::Text,
                    true,
                    "Comma-separated files or globs, relative to the card's worktree (e.g. \
                     logs/app.log, logs/*.log) or absolute.",
                )],
            ),
            "https://github.com/BurntSushi/ripgrep/tree/master/crates/globset",
        )
    }

    pub fn new(settings: &SourceSettings, root: Option<PathBuf>) -> Result<Self, EvidenceError> {
        let patterns = settings.list("paths");
        if patterns.is_empty() {
            return Err(EvidenceError::NotConfigured(
                "at least one log file path".to_string(),
            ));
        }
        for pattern in &patterns {
            matcher(pattern)?;
        }
        Ok(Self {
            settings: settings.clone(),
            patterns,
            root,
        })
    }
}

fn matcher(pattern: &str) -> Result<GlobMatcher, EvidenceError> {
    // `*` stays within one directory, as in a shell; `**` crosses them.
    GlobBuilder::new(pattern)
        .literal_separator(true)
        .build()
        .map(|glob| glob.compile_matcher())
        .map_err(|e| EvidenceError::Invalid(format!("{pattern:?} is not a valid glob: {e}")))
}

fn has_glob(segment: &str) -> bool {
    segment.contains(['*', '?', '[', '{'])
}

/// The files `pattern` names under `root` (or as written, when absolute),
/// sorted, at most [`FILE_CAP`].
fn expand(pattern: &str, root: Option<&Path>) -> Result<Vec<PathBuf>, EvidenceError> {
    let absolute = Path::new(pattern).is_absolute();
    let base = match (absolute, root) {
        (true, _) => PathBuf::new(),
        (false, Some(root)) => root.to_path_buf(),
        (false, None) => return Ok(Vec::new()),
    };
    if !has_glob(pattern) {
        let path = base.join(pattern);
        return Ok(if path.is_file() {
            vec![path]
        } else {
            Vec::new()
        });
    }
    // Walk from the deepest directory the pattern names literally.
    let mut literal = PathBuf::new();
    for segment in pattern.split('/') {
        if has_glob(segment) {
            break;
        }
        literal.push(if segment.is_empty() { "/" } else { segment });
    }
    let start = base.join(&literal);
    if !start.is_dir() {
        return Ok(Vec::new());
    }
    let glob = matcher(pattern)?;
    let mut files: Vec<PathBuf> = ignore::WalkBuilder::new(&start)
        .standard_filters(false)
        .follow_links(false)
        .max_depth(Some(WALK_DEPTH))
        .build()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_some_and(|kind| kind.is_file()))
        .map(|entry| entry.into_path())
        .filter(|path| {
            let relative = if absolute {
                path.as_path()
            } else {
                path.strip_prefix(&base).unwrap_or(path)
            };
            glob.is_match(relative)
        })
        .collect();
    files.sort();
    files.truncate(FILE_CAP);
    Ok(files)
}

/// A timestamp written as text: RFC 3339, or a date and time without a zone,
/// read as UTC.
fn parse_time_text(raw: &str) -> Option<DateTime<Utc>> {
    let raw = raw
        .trim()
        .trim_matches(|c| matches!(c, '[' | ']' | ',' | '"'));
    if let Ok(at) = DateTime::parse_from_rfc3339(raw) {
        return Some(at.with_timezone(&Utc));
    }
    ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%d %H:%M:%S%.f"]
        .into_iter()
        .find_map(|format| NaiveDateTime::parse_from_str(raw, format).ok())
        .map(|at| at.and_utc())
}

/// Epoch seconds or milliseconds, told apart by size.
fn parse_epoch(value: f64) -> Option<DateTime<Utc>> {
    if value > 1e12 {
        DateTime::from_timestamp_millis(value as i64)
    } else if value > 1e9 {
        DateTime::from_timestamp(value as i64, 0)
    } else {
        None
    }
}

/// When a log line says it was written: a JSON line's `time` (or
/// `timestamp`, `ts`, …) field, else a timestamp at the start of the line.
pub(crate) fn line_time(line: &str) -> Option<DateTime<Utc>> {
    let line = line.trim_start();
    if line.starts_with('{')
        && let Ok(Value::Object(object)) = serde_json::from_str::<Value>(line)
    {
        return TIME_KEYS.iter().find_map(|key| match object.get(*key)? {
            Value::String(text) => parse_time_text(text),
            Value::Number(number) => parse_epoch(number.as_f64()?),
            _ => None,
        });
    }
    let mut tokens = line.split_whitespace();
    let first = tokens.next()?;
    parse_time_text(first).or_else(|| {
        let second = tokens.next()?;
        parse_time_text(&format!("{} {}", first.trim_start_matches('['), second))
    })
}

/// The last [`READ_CAP`] bytes of a file as text, and whether it was cut.
fn read_tail(path: &Path) -> std::io::Result<(String, bool)> {
    let mut file = std::fs::File::open(path)?;
    let length = file.metadata()?.len();
    let cut = length > READ_CAP;
    if cut {
        file.seek(SeekFrom::Start(length - READ_CAP))?;
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    let mut text = String::from_utf8_lossy(&bytes).into_owned();
    if cut {
        // The first line is partial.
        text = text
            .split_once('\n')
            .map(|(_, rest)| rest.to_string())
            .unwrap_or_default();
    }
    Ok((text, cut))
}

/// What one search found in the files, newest lines last within a file.
fn search_files(
    files: &[(PathBuf, String)],
    query: &SearchQuery,
    label: &str,
) -> Result<(Vec<Hit>, bool), EvidenceError> {
    let needle = query.pattern.trim().to_lowercase();
    let mut hits = Vec::new();
    let mut more = false;
    for (path, shown) in files {
        let (text, cut) = read_tail(path)
            .map_err(|e| EvidenceError::Api(format!("could not read {shown}: {e}")))?;
        let mut found: std::collections::VecDeque<Hit> = std::collections::VecDeque::new();
        for (index, line) in text.lines().enumerate() {
            if line.trim().is_empty() || !line.to_lowercase().contains(&needle) {
                continue;
            }
            let at = line_time(line);
            if at.is_some_and(|at| !query.contains(at)) {
                continue;
            }
            let mut hit = Hit::new(label, at, line).field("file", shown.as_str());
            if !cut {
                hit = hit.field("line", index + 1);
            }
            found.push_back(hit);
            if found.len() > query.limit {
                found.pop_front();
                more = true;
            }
        }
        // Newest (last written) first.
        hits.extend(found.into_iter().rev());
    }
    Ok((hits, more))
}

/// The paragraph a `log_files` source gets in `{{product.evidence_tools}}`.
/// Kept separate from the source so the toolkit's generated skills can be
/// checked against it.
pub fn describe_paths(heading: &str, label: &str, paths: &[String], notes: Option<&str>) -> String {
    let shown = paths
        .iter()
        .map(|path| format!("`{path}`"))
        .collect::<Vec<_>>()
        .join(", ");
    let relative = if paths.iter().any(|path| !Path::new(path).is_absolute()) {
        ", relative to the worktree"
    } else {
        ""
    };
    let pronoun = if paths.len() == 1 && !has_glob(&paths[0]) {
        "it"
    } else {
        "them"
    };
    let mut out = format!(
        "{heading}: {shown}{relative}. Read {pronoun} with tail, grep or jq, or call \
         `evidence_search` with source \"{label}\": it returns the lines that contain the \
         pattern, ignoring case, leaving out a line whose timestamp falls outside the window and \
         keeping one that has none. Look for the failing request and the error logged after it, \
         around the time you ran the harness."
    );
    if let Some(notes) = notes {
        out.push(' ');
        out.push_str(notes);
    }
    out
}

#[async_trait]
impl EvidenceSource for LogFilesSource {
    fn kind(&self) -> &'static str {
        EvidenceKind::LogFiles.as_str()
    }

    fn label(&self) -> String {
        self.settings.label()
    }

    fn describe(&self) -> String {
        describe_paths(
            &self.settings.heading(),
            &self.label(),
            &self.patterns,
            self.settings.notes().as_deref(),
        )
    }

    fn mcp_server(&self) -> Option<McpServerEntry> {
        None
    }

    async fn search(&self, query: &SearchQuery) -> Result<Option<SearchResult>, EvidenceError> {
        let root = query.worktree.clone().or_else(|| self.root.clone());
        let patterns = self.patterns.clone();
        let label = self.label();
        let query = query.clone();
        tokio::task::spawn_blocking(move || {
            let mut files: Vec<(PathBuf, String)> = Vec::new();
            for pattern in &patterns {
                if root.is_none() && !Path::new(pattern).is_absolute() {
                    return Err(EvidenceError::Invalid(format!(
                        "{pattern} is relative to a worktree, and this search names none"
                    )));
                }
                for path in expand(pattern, root.as_deref())? {
                    if files.iter().any(|(seen, _)| *seen == path) {
                        continue;
                    }
                    let shown = root
                        .as_deref()
                        .and_then(|root| path.strip_prefix(root).ok())
                        .unwrap_or(&path)
                        .display()
                        .to_string();
                    files.push((path, shown));
                }
            }
            let (hits, more) = search_files(&files, &query, &label)?;
            Ok(Some(capped(hits, query.limit, more)))
        })
        .await
        .map_err(|e| EvidenceError::Api(format!("the log search stopped: {e}")))?
    }

    async fn test(&self) -> Result<String, EvidenceError> {
        let (relative, absolute): (Vec<&String>, Vec<&String>) = self
            .patterns
            .iter()
            .partition(|pattern| !Path::new(pattern.as_str()).is_absolute());
        let mut found = 0;
        for pattern in &absolute {
            found += expand(pattern, None)?.len();
        }
        if let Some(root) = &self.root {
            for pattern in &relative {
                found += expand(pattern, Some(root))?.len();
            }
        }
        let checked = absolute.len()
            + if self.root.is_some() {
                relative.len()
            } else {
                0
            };
        if checked == 0 {
            return Ok(format!(
                "{} relative path(s); they are read in each card's worktree, so there is \
                 nothing to check here",
                relative.len()
            ));
        }
        if found == 0 {
            return Err(EvidenceError::Api(
                "no file matches these paths yet; the app may not have written one".to_string(),
            ));
        }
        Ok(format!("{found} file(s) match"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::evidence::test_support::*;

    fn source(paths: &str, root: Option<&Path>) -> LogFilesSource {
        LogFilesSource::new(
            &settings(
                EvidenceKind::LogFiles,
                &[("label", "App log"), ("paths", paths)],
            ),
            root.map(Path::to_path_buf),
        )
        .unwrap()
    }

    fn worktree() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("logs/old")).unwrap();
        std::fs::write(
            dir.path().join("logs/app.log"),
            [
                r#"{"time":"2026-10-17T23:00:00.000Z","level":"error","message":"SeedMissing early"}"#,
                r#"{"time":"2026-10-18T09:00:00.000Z","level":"info","path":"/lists"}"#,
                r#"{"time":"2026-10-18T10:00:00.000Z","level":"error","message":"SeedMissing: list 7"}"#,
                "    at listRoute (src/routes/lists.js:42:11) seedmissing frame",
                r#"{"time":"2026-10-18T11:00:00.000Z","level":"error","message":"SeedMissing: list 8"}"#,
            ]
            .join("\n"),
        )
        .unwrap();
        std::fs::write(
            dir.path().join("logs/old/worker.log"),
            "2026-10-18 10:30:00 worker SeedMissing in job 3\n",
        )
        .unwrap();
        dir
    }

    #[tokio::test]
    async fn search_keeps_the_window_and_lines_without_a_time() {
        let dir = worktree();
        let logs = source("logs/app.log", None);
        let mut query = query(
            "seedmissing",
            "2026-10-18T00:00:00Z",
            "2026-10-18T23:59:59Z",
            10,
        );
        query.worktree = Some(dir.path().to_path_buf());
        let result = logs.search(&query).await.unwrap().unwrap();
        let lines: Vec<(Option<usize>, Option<&str>)> = result
            .hits
            .iter()
            .map(|h| {
                (
                    h.fields["line"].as_u64().map(|n| n as usize),
                    h.fields["file"].as_str(),
                )
            })
            .collect();
        // Line 1 is before the window; line 2 does not match; line 4 has no
        // time and stays, after the dated ones.
        assert_eq!(
            lines,
            [
                (Some(5), Some("logs/app.log")),
                (Some(3), Some("logs/app.log")),
                (Some(4), Some("logs/app.log")),
            ]
        );
        assert_eq!(result.hits[0].at, Some(at("2026-10-18T11:00:00Z")));
        assert_eq!(result.hits[2].at, None);
        assert!(!result.truncated);

        query.limit = 1;
        let capped = logs.search(&query).await.unwrap().unwrap();
        assert_eq!(capped.hits.len(), 1);
        assert!(capped.truncated);
        assert_eq!(capped.hits[0].fields["line"], serde_json::json!(5));
    }

    #[tokio::test]
    async fn globs_expand_under_the_root_and_relative_paths_need_one() {
        let dir = worktree();
        let logs = source("logs/**/*.log", Some(dir.path()));
        let result = logs
            .search(&query(
                "SeedMissing in job",
                "2026-10-18T00:00:00Z",
                "2026-10-18T23:59:59Z",
                10,
            ))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(result.hits.len(), 1);
        assert_eq!(result.hits[0].fields["file"], "logs/old/worker.log");
        assert_eq!(result.hits[0].at, Some(at("2026-10-18T10:30:00Z")));

        let rootless = source("logs/app.log", None);
        assert!(matches!(
            rootless
                .search(&query(
                    "x",
                    "2026-10-18T00:00:00Z",
                    "2026-10-18T01:00:00Z",
                    1
                ))
                .await,
            Err(EvidenceError::Invalid(_))
        ));
    }

    #[tokio::test]
    async fn the_test_counts_files_it_can_check() {
        let dir = worktree();
        assert_eq!(
            source("logs/*.log", Some(dir.path())).test().await.unwrap(),
            "1 file(s) match"
        );
        assert!(
            source("logs/app.log", None)
                .test()
                .await
                .unwrap()
                .contains("read in each card's worktree")
        );
        let absolute = dir.path().join("logs/missing.log");
        assert!(
            source(absolute.to_str().unwrap(), None)
                .test()
                .await
                .is_err()
        );
        assert!(LogFilesSource::new(&settings(EvidenceKind::LogFiles, &[]), None).is_err());
        assert!(
            LogFilesSource::new(
                &settings(EvidenceKind::LogFiles, &[("paths", "logs/[.log")]),
                None
            )
            .is_err()
        );
    }

    #[test]
    fn line_times_come_from_json_fields_or_the_line_start() {
        assert_eq!(
            line_time(r#"{"level":"info","time":"2026-10-18T10:00:00Z"}"#),
            Some(at("2026-10-18T10:00:00Z"))
        );
        assert_eq!(
            line_time(r#"{"ts":1792317600.5}"#),
            Some(at("2026-10-18T10:00:00Z"))
        );
        assert_eq!(
            line_time(r#"{"@timestamp":1792317600000}"#),
            Some(at("2026-10-18T10:00:00Z"))
        );
        assert_eq!(
            line_time("2026-10-18T10:00:00+02:00 GET /"),
            Some(at("2026-10-18T08:00:00Z"))
        );
        assert_eq!(
            line_time("[2026-10-18 10:00:00.250] worker"),
            Some(at("2026-10-18T10:00:00.250Z"))
        );
        assert_eq!(line_time("    at frame (file.js:1:1)"), None);
        assert_eq!(line_time(r#"{"message":"no time"}"#), None);
    }

    #[test]
    fn describe_snapshot() {
        let logs = LogFilesSource::new(
            &settings(
                EvidenceKind::LogFiles,
                &[
                    ("label", "Pantry app log"),
                    ("paths", "logs/app.log"),
                    ("notes", "One JSON line per request"),
                ],
            ),
            None,
        )
        .unwrap();
        assert_eq!(
            logs.describe(),
            "Pantry app log (Log files): `logs/app.log`, relative to the worktree. Read it with \
             tail, grep or jq, or call `evidence_search` with source \"Pantry app log\": it \
             returns the lines that contain the pattern, ignoring case, leaving out a line whose \
             timestamp falls outside the window and keeping one that has none. Look for the \
             failing request and the error logged after it, around the time you ran the harness. \
             One JSON line per request."
        );
        let several = LogFilesSource::new(
            &settings(EvidenceKind::LogFiles, &[("paths", "/var/log/app/*.log")]),
            None,
        )
        .unwrap();
        assert!(
            several
                .describe()
                .starts_with("Log files: `/var/log/app/*.log`. Read them with")
        );
        assert!(several.mcp_server().is_none());
    }
}

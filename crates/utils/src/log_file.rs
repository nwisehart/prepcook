//! On-disk log destination for the app's tracing output.
//!
//! GUI launches (Dock / Finder) inherit stdout pointing at `/dev/null`, so
//! everything tracing emits is discarded — two desktop-only bugs cost hours
//! of inference each because no log survived the run. A plain file next to
//! the app's other state fixes that; terminal launches keep their stdout
//! layer as well.
//!
//! Rotation is deliberately trivial (one previous generation, size-capped at
//! open time) so this needs no extra dependency and no background task.

use std::{fs::File, io, path::PathBuf};

/// Bytes after which the current log is rotated to `<name>.1` on the next
/// start. One generation is enough to keep the run *before* a crash.
const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;

/// Directory holding app logs (created on demand).
pub fn log_dir() -> PathBuf {
    crate::assets::asset_dir().join("logs")
}

/// Path of the current log file for `name` (e.g. `"desktop"`, `"server"`).
pub fn log_path(name: &str) -> PathBuf {
    log_dir().join(format!("{name}.log"))
}

/// Opens the log file for appending, rotating first when it has grown past
/// [`MAX_LOG_BYTES`]. Returns `None` when the file can't be opened — logging
/// to disk is a convenience and must never prevent startup.
pub fn open_log_file(name: &str) -> Option<File> {
    let dir = log_dir();
    if let Err(error) = std::fs::create_dir_all(&dir) {
        eprintln!("could not create log dir {}: {error}", dir.display());
        return None;
    }
    let current = log_path(name);
    if let Ok(metadata) = std::fs::metadata(&current)
        && metadata.len() > MAX_LOG_BYTES
    {
        let previous = dir.join(format!("{name}.log.1"));
        let _ = std::fs::rename(&current, &previous);
    }
    match File::options().create(true).append(true).open(&current) {
        Ok(file) => Some(file),
        Err(error) => {
            eprintln!("could not open log file {}: {error}", current.display());
            None
        }
    }
}

/// Best-effort tail of a log file, for surfacing recent errors in-app or in
/// a support bundle. Reads at most `max_bytes` from the end.
pub fn tail(name: &str, max_bytes: u64) -> io::Result<String> {
    use std::io::{Read, Seek, SeekFrom};

    let mut file = File::open(log_path(name))?;
    let len = file.metadata()?.len();
    let start = len.saturating_sub(max_bytes);
    file.seek(SeekFrom::Start(start))?;
    let mut buffer = String::new();
    file.read_to_string(&mut buffer)?;
    // A mid-character seek can leave a partial first line; drop it.
    if start > 0
        && let Some(newline) = buffer.find('\n')
    {
        buffer.drain(..=newline);
    }
    Ok(buffer)
}

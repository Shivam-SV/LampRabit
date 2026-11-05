//! Collects log output for the Logs page: service stdout/stderr plus the log files the
//! services write themselves (Apache error/access, MariaDB, PHP), tailed by polling.

use std::{
    collections::{HashMap, VecDeque},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::Mutex,
};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

/// Lines kept in memory for pages that open later.
const KEEP: usize = 5000;
/// Upper bound on bytes read from one file per poll.
const MAX_READ: u64 = 1024 * 1024;

/// (file name in logs/, service, stream)
const SOURCES: [(&str, &str, &str); 8] = [
    ("apache-error.log", "apache", "error"),
    ("apache-access.log", "apache", "access"),
    ("nginx-error.log", "nginx", "error"),
    ("nginx-access.log", "nginx", "access"),
    ("mariadb-error.log", "mariadb", "error"),
    ("mysql-error.log", "mysql", "error"),
    ("php-error.log", "php", "error"),
    ("php-fpm.log", "php", "error"),
];

#[derive(Debug, Clone, Serialize)]
pub struct LogLine {
    pub id: u64,
    pub service: &'static str,
    pub stream: &'static str,
    pub line: String,
    /// For service "project": which project (its Node app and the commands run for it).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
}

#[derive(Default)]
pub struct LogHub {
    inner: Mutex<(u64, VecDeque<LogLine>)>,
    /// Where system events are persisted (`logs/lampRabit.log`).
    system_file: Mutex<Option<PathBuf>>,
}

pub fn set_system_file(app: &AppHandle, path: PathBuf) {
    *app.state::<LogHub>().system_file.lock().unwrap() = Some(path);
}

#[derive(Debug, Clone, Copy)]
pub enum Level {
    Info,
    Warn,
    Error,
}

/// Records something LampRabit did (service lifecycle, installs, sites, ...)
/// in the System log stream and in `logs/lampRabit.log`.
/// Local time as log files write it: `2026-10-31 12:00:00`.
pub fn stamp() -> String {
    let now = time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());
    now.format(time::macros::format_description!("[year]-[month]-[day] [hour]:[minute]:[second]")).unwrap_or_default()
}

pub fn system(app: &AppHandle, level: Level, message: impl Into<String>) {
    let stamp = stamp();
    let level = match level {
        Level::Info => "info",
        Level::Warn => "warn",
        Level::Error => "error",
    };
    let line = format!("[{stamp}] {level}: {}", message.into());

    let file = app.state::<LogHub>().system_file.lock().unwrap().clone();
    if let Some(file) = file {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(file) {
            let _ = writeln!(f, "{line}");
        }
    }
    push(app, "system", "output", line);
}

pub fn push(app: &AppHandle, service: &'static str, stream: &'static str, line: String) {
    let hub = app.state::<LogHub>();
    let entry = {
        let mut guard = hub.inner.lock().unwrap();
        let (seq, lines) = &mut *guard;
        *seq += 1;
        let entry = LogLine { id: *seq, service, stream, line, project: None };
        lines.push_back(entry.clone());
        if lines.len() > KEEP {
            lines.pop_front();
        }
        entry
    };
    let _ = app.emit("log-line", entry);
}

/// A line of a project's output (its Node app or a command run for it), for the Logs page.
/// `stream` is "output", "error" or "command" (the command line itself).
pub fn push_project(app: &AppHandle, project: &str, stream: &'static str, line: String) {
    let hub = app.state::<LogHub>();
    let entry = {
        let mut guard = hub.inner.lock().unwrap();
        let (seq, lines) = &mut *guard;
        *seq += 1;
        let entry = LogLine { id: *seq, service: "project", stream, line, project: Some(project.to_string()) };
        lines.push_back(entry.clone());
        if lines.len() > KEEP {
            lines.pop_front();
        }
        entry
    };
    let _ = app.emit("log-line", entry);
}

/// A caught dump's one-line summary (dumps.rs), in logs/dumps.log and the Logs page.
pub fn push_dump(app: &AppHandle, project: Option<String>, line: String) {
    let file = app.state::<LogHub>().system_file.lock().unwrap().clone();
    if let Some(dir) = file.as_deref().and_then(Path::parent) {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("dumps.log")) {
            let _ = writeln!(f, "{line}");
        }
    }
    let hub = app.state::<LogHub>();
    let entry = {
        let mut guard = hub.inner.lock().unwrap();
        let (seq, lines) = &mut *guard;
        *seq += 1;
        let entry = LogLine { id: *seq, service: "dumps", stream: "output", line, project };
        lines.push_back(entry.clone());
        if lines.len() > KEEP {
            lines.pop_front();
        }
        entry
    };
    let _ = app.emit("log-line", entry);
}

/// "apache-error.1.log": an older copy kept by `rotate`, not a log of its own.
fn rotated(path: &Path) -> bool {
    path.file_stem().and_then(|s| s.to_str()).and_then(|s| s.rsplit_once('.')).is_some_and(|(_, n)| n.parse::<u32>().is_ok())
}

/// Projects' log files (logs/apps/<project>.log), written by apps.rs. Each line starts with
/// its time and stream: `[2026-10-31 12:00:00] error: …`.
pub fn project_dir(dir: &Path) -> PathBuf {
    dir.join("apps")
}

/// Files written by LampRabit itself: the system log and captured service console output.
const OWN_FILES: [(&str, &str, &str); 6] = [
    ("lampRabit.log", "system", "output"),
    ("dumps.log", "dumps", "output"),
    ("apache-output.log", "apache", "output"),
    ("mariadb-output.log", "mariadb", "output"),
    ("mysql-output.log", "mysql", "output"),
    ("nginx-output.log", "nginx", "output"),
];
/// How much of each file the Logs page loads.
const HISTORY_BYTES: u64 = 192 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct History {
    pub lines: Vec<LogLine>,
    /// Live lines with an id above this arrived after the snapshot.
    pub cursor: u64,
}

/// Reads the recent part of every log file from disk, so history survives restarts.
pub fn history(app: &AppHandle, dir: &Path) -> History {
    let cursor = app.state::<LogHub>().inner.lock().unwrap().0;
    let mut lines = Vec::new();
    for (file, service, stream) in SOURCES.iter().chain(OWN_FILES.iter()) {
        let path = dir.join(file);
        let Ok(len) = std::fs::metadata(&path).map(|m| m.len()) else { continue };
        let from = len.saturating_sub(HISTORY_BYTES);
        let Ok(bytes) = read_range(&path, from, len - from) else { continue };
        let text = String::from_utf8_lossy(&bytes);
        let mut iter = text.split('\n');
        if from > 0 {
            iter.next(); // partial first line
        }
        lines.extend(
            iter.map(|l| l.trim_end_matches('\r'))
                .filter(|l| !l.is_empty())
                .map(|l| LogLine { id: 0, service, stream, line: l.to_string(), project: None }),
        );
    }
    // Projects' output, one file each.
    for entry in std::fs::read_dir(project_dir(dir)).into_iter().flatten().flatten() {
        let path = entry.path();
        if rotated(&path) {
            continue;
        }
        let Some(project) = path.file_stem().and_then(|n| n.to_str()).filter(|_| path.extension().is_some_and(|e| e == "log")) else { continue };
        let Ok(len) = std::fs::metadata(&path).map(|m| m.len()) else { continue };
        let from = len.saturating_sub(HISTORY_BYTES);
        let Ok(bytes) = read_range(&path, from, len - from) else { continue };
        let text = String::from_utf8_lossy(&bytes);
        let mut iter = text.split('\n');
        if from > 0 {
            iter.next();
        }
        for l in iter.map(|l| l.trim_end_matches('\r')).filter(|l| !l.is_empty()) {
            let stream = if l.contains("] error: ") { "error" } else if l.contains("] command: ") { "command" } else { "output" };
            lines.push(LogLine { id: 0, service: "project", stream, line: l.to_string(), project: Some(project.to_string()) });
        }
    }
    History { lines, cursor }
}

/// Empties the log files of one source ("apache", "dumps", "project:shop"), or all of them
/// with None. A project's processes ("shop/queue", kept as shop~queue.log) go with it.
/// Returns the files that could not be cleared.
pub fn clear(app: &AppHandle, dir: &Path, source: Option<&str>) -> Vec<String> {
    let project = source.and_then(|s| s.strip_prefix("project:"));
    {
        let hub = app.state::<LogHub>();
        let mut guard = hub.inner.lock().unwrap();
        match source {
            None => guard.1.clear(),
            Some(_) if project.is_some() => guard.1.retain(|l| !(l.service == "project" && l.project.as_deref().is_some_and(|p| Some(p.split('/').next().unwrap_or(p)) == project))),
            Some(s) => guard.1.retain(|l| l.service != s),
        }
    }
    let empty = |path: &Path| std::fs::OpenOptions::new().write(true).open(path).and_then(|f| f.set_len(0)).is_ok();
    let mut failed = Vec::new();
    if project.is_none() {
        for (file, service, _) in SOURCES.iter().chain(OWN_FILES.iter()) {
            let path = dir.join(file);
            if source.is_some_and(|s| s != *service) || !path.exists() {
                continue;
            }
            if !empty(&path) {
                failed.push(file.to_string());
            }
        }
    }
    if source.is_none() || project.is_some() {
        for entry in std::fs::read_dir(project_dir(dir)).into_iter().flatten().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let stem = name.strip_suffix(".log").unwrap_or(&name);
            if project.is_some_and(|p| stem != p && !stem.starts_with(&format!("{p}~"))) {
                continue;
            }
            if !empty(&entry.path()) {
                failed.push(format!("apps/{name}"));
            }
        }
    }
    failed
}

/// Keeps log files to `max_bytes`: a bigger one is copied to name.1.log (older copies move
/// up to name.<keep>.log, the oldest is deleted) and emptied in place. Copy-then-truncate,
/// because services keep their log open and Windows won't rename an open file; a file that
/// can't be emptied right now is tried again next time. Then, while the folder is over
/// `total_bytes`, the oldest copies go. Returns how many files were rotated.
pub fn rotate(dir: &Path, max_bytes: u64, keep: u32, total_bytes: u64) -> usize {
    let mut rotated_count = 0;
    let mut files: Vec<PathBuf> = Vec::new();
    for folder in [dir.to_path_buf(), project_dir(dir)] {
        for entry in std::fs::read_dir(&folder).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "log") && !rotated(&path) {
                files.push(path);
            }
        }
    }
    for path in &files {
        let Ok(len) = std::fs::metadata(path).map(|m| m.len()) else { continue };
        if len <= max_bytes {
            continue;
        }
        let copy = |n: u32| path.with_extension(format!("{n}.log"));
        if keep > 0 {
            let _ = std::fs::remove_file(copy(keep));
            for n in (1..keep).rev() {
                let _ = std::fs::rename(copy(n), copy(n + 1));
            }
            if std::fs::copy(path, copy(1)).is_err() {
                continue;
            }
        }
        if std::fs::OpenOptions::new().write(true).open(path).and_then(|f| f.set_len(0)).is_ok() {
            rotated_count += 1;
        }
    }
    // The folder as a whole: oldest copies first.
    let mut copies: Vec<(std::time::SystemTime, u64, PathBuf)> = Vec::new();
    let mut total = 0u64;
    for folder in [dir.to_path_buf(), project_dir(dir)] {
        for entry in std::fs::read_dir(&folder).into_iter().flatten().flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            if !meta.is_file() {
                continue;
            }
            total += meta.len();
            if rotated(&entry.path()) {
                copies.push((meta.modified().unwrap_or(std::time::UNIX_EPOCH), meta.len(), entry.path()));
            }
        }
    }
    // Oldest first. Copies made within the same moment tie on time (a test, or a log
    // rotated twice in quick succession), and the folder's order differs per OS, so the
    // copy number decides then: name.2.log is older than name.1.log.
    let number = |p: &Path| p.file_stem().and_then(|s| s.to_str()).and_then(|s| s.rsplit_once('.')).and_then(|(_, n)| n.parse::<u32>().ok()).unwrap_or(0);
    copies.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| number(&b.2).cmp(&number(&a.2))));
    for (_, len, path) in copies {
        if total <= total_bytes {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(len);
        }
    }
    rotated_count
}

/// Per-file read position and any trailing partial line. Existing content is read by
/// `history`, so files present at the first poll are followed from their end.
#[derive(Default)]
pub struct Tailer {
    files: HashMap<PathBuf, (u64, Vec<u8>)>,
    /// The logs directory of the first poll; changes when the environment root moves.
    dir: Option<PathBuf>,
}

impl Tailer {
    pub fn poll(&mut self, app: &AppHandle, dir: &Path) {
        let first_poll = self.dir.as_deref() != Some(dir);
        if first_poll {
            self.files.clear();
            self.dir = Some(dir.to_path_buf());
        }
        for (file, service, stream) in SOURCES {
            let path = dir.join(file);
            let Ok(len) = std::fs::metadata(&path).map(|m| m.len()) else { continue };
            // Files that appear later are new, so they are read from the start.
            let entry = self
                .files
                .entry(path.clone())
                .or_insert_with(|| (if first_poll { len } else { 0 }, Vec::new()));
            if len < entry.0 {
                *entry = (0, Vec::new()); // truncated or replaced
            }
            if len == entry.0 {
                continue;
            }
            let Ok(chunk) = read_range(&path, entry.0, (len - entry.0).min(MAX_READ)) else { continue };
            entry.0 += chunk.len() as u64;
            let mut bytes = std::mem::take(&mut entry.1);
            bytes.extend_from_slice(&chunk);

            let mut lines: Vec<&[u8]> = bytes.split(|&b| b == b'\n').collect();
            let rest = lines.pop().unwrap_or_default().to_vec();
            for line in lines {
                let text = String::from_utf8_lossy(line).trim_end_matches('\r').to_string();
                if !text.is_empty() {
                    push(app, service, stream, text);
                }
            }
            entry.1 = rest;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotates_big_files_and_keeps_copies() {
        let dir = std::env::temp_dir().join(format!("lamprabit-rotate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(project_dir(&dir)).unwrap();
        let log = dir.join("apache-error.log");
        for round in 0..4 {
            std::fs::write(&log, vec![b'x'; 2000 + round]).unwrap();
            assert_eq!(rotate(&dir, 1000, 2, u64::MAX), 1);
            assert_eq!(std::fs::metadata(&log).unwrap().len(), 0, "emptied in place");
        }
        assert!(dir.join("apache-error.1.log").exists() && dir.join("apache-error.2.log").exists());
        assert!(!dir.join("apache-error.3.log").exists(), "only `keep` copies");
        assert_eq!(std::fs::metadata(dir.join("apache-error.1.log")).unwrap().len(), 2003, "newest copy first");
        // Over the folder's total, the oldest copies go.
        rotate(&dir, 1000, 2, 2500);
        assert!(!dir.join("apache-error.2.log").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

fn read_range(path: &Path, from: u64, len: u64) -> std::io::Result<Vec<u8>> {
    let mut file = std::fs::File::open(path)?;
    file.seek(SeekFrom::Start(from))?;
    let mut buf = Vec::with_capacity(len as usize);
    file.take(len).read_to_end(&mut buf)?;
    Ok(buf)
}

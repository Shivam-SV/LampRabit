//! Moving the LampRabit folder (the environment root) somewhere else: the folders LampRabit
//! owns and the user's (www, data, backups, templates) go together, renamed on the same
//! drive and copied, checked and then deleted across drives. A failure puts back what was
//! already moved, so the old folder keeps working. Configs are regenerated from the new
//! root when services start; the few things that store absolute paths (Python virtual
//! environments, PATH, shims) are fixed here.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::error::{Error, Result};

/// Everything that moves: what uninstall removes and what it keeps.
pub fn folders() -> Vec<&'static str> {
    crate::uninstall::OWNED.iter().chain(crate::uninstall::KEPT.iter()).copied().collect()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveCheck {
    /// Why the target can't be used; empty when it can.
    pub problems: Vec<String>,
    /// Bytes to move.
    pub size: u64,
    /// Free bytes on the target's drive, when known.
    pub free: Option<u64>,
    /// A rename (instant) rather than a copy.
    pub same_drive: bool,
    pub file_system: Option<String>,
}

/// A process that holds files in the folder: it runs from it or works inside it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Holder {
    pub pid: u32,
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    done: u64,
    total: u64,
    current: String,
}

fn size_of(path: &Path) -> u64 {
    let Ok(meta) = std::fs::symlink_metadata(path) else { return 0 };
    if meta.is_dir() {
        std::fs::read_dir(path).into_iter().flatten().flatten().map(|e| size_of(&e.path())).sum()
    } else {
        meta.len()
    }
}

/// The drive (mount point) a path is on: the disk whose mount point is its longest prefix.
fn disk_of<'a>(disks: &'a sysinfo::Disks, path: &Path) -> Option<&'a sysinfo::Disk> {
    let path = dunce_canonical(path);
    disks.list().iter().filter(|d| path.starts_with(d.mount_point())).max_by_key(|d| d.mount_point().as_os_str().len())
}

/// Canonical path without Windows' \\?\ prefix (so it compares with mount points), falling
/// back to the nearest existing parent for a folder that doesn't exist yet.
fn dunce_canonical(path: &Path) -> PathBuf {
    let mut probe = path.to_path_buf();
    let mut rest = Vec::new();
    loop {
        if let Ok(c) = probe.canonicalize() {
            let text = c.to_string_lossy().into_owned();
            let mut c = PathBuf::from(text.strip_prefix(r"\\?\").unwrap_or(&text));
            for part in rest.iter().rev() {
                c.push(part);
            }
            return c;
        }
        match (probe.file_name().map(|n| n.to_os_string()), probe.parent()) {
            (Some(name), Some(parent)) => {
                rest.push(name);
                probe = parent.to_path_buf();
            }
            _ => return path.to_path_buf(),
        }
    }
}

/// Linux mount options for the mount a path is on (from /proc/mounts).
#[cfg(target_os = "linux")]
fn mount_options(path: &Path) -> Option<String> {
    let mounts = std::fs::read_to_string("/proc/mounts").ok()?;
    let path = dunce_canonical(path);
    mounts
        .lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            (f.len() >= 4).then(|| (f[1].replace("\\040", " "), f[3].to_string()))
        })
        .filter(|(mount, _)| path.starts_with(mount))
        .max_by_key(|(mount, _)| mount.len())
        .map(|(_, options)| options)
}

fn system_folder(path: &Path) -> bool {
    let lower = path.to_string_lossy().to_lowercase().replace('\\', "/");
    let lower = lower.trim_end_matches('/');
    if cfg!(windows) {
        ["SystemRoot", "ProgramFiles", "ProgramFiles(x86)", "ProgramData"]
            .iter()
            .filter_map(|v| std::env::var(v).ok())
            .map(|v| v.to_lowercase().replace('\\', "/"))
            .any(|s| lower == s || lower.starts_with(&format!("{s}/")))
            // A drive's root (C:\) holds the system's own folders.
            || lower.len() <= 2
    } else {
        lower.is_empty() || ["/bin", "/boot", "/dev", "/etc", "/lib", "/proc", "/sbin", "/sys", "/usr", "/var", "/system", "/library", "/applications"]
            .iter()
            .any(|s| lower == *s || lower.starts_with(&format!("{s}/")))
    }
}

/// Whether `target` can hold the LampRabit folder, and what moving there involves.
pub fn check(current: &Path, target: &Path) -> MoveCheck {
    let mut problems = Vec::new();
    let names = folders();
    let size: u64 = names.iter().map(|n| size_of(&current.join(n))).sum();
    let mut out = MoveCheck { problems: Vec::new(), size, free: None, same_drive: false, file_system: None };
    if !target.is_absolute() {
        out.problems.push("Choose a full path, for example D:\\LampRabit or /home/you/LampRabit.".into());
        return out;
    }
    let (cur, tgt) = (dunce_canonical(current), dunce_canonical(target));
    if cur == tgt {
        out.problems.push("That's where LampRabit already is.".into());
        return out;
    }
    if tgt.starts_with(&cur) {
        problems.push("The new folder can't be inside the current one.".into());
    }
    if cur.starts_with(&tgt) {
        problems.push("The new folder can't contain the current one; choose an empty or new folder.".into());
    }
    if system_folder(&tgt) {
        problems.push("That's a system folder (or a drive's root). Choose a folder of your own, such as D:\\LampRabit.".into());
    }
    let taken: Vec<&str> = names
        .iter()
        .copied()
        .filter(|n| std::fs::read_dir(target.join(n)).is_ok_and(|mut d| d.next().is_some()))
        .collect();
    if !taken.is_empty() {
        problems.push(format!("The folder already has {} in it. Choose an empty or new folder.", taken.join(", ")));
    }
    // Writes, for real: some locations accept creating a folder but not files. A folder
    // made just for this test is removed again, so looking leaves nothing behind.
    let existed = target.exists();
    match std::fs::create_dir_all(target) {
        Ok(()) => {
            let probe = target.join(".lamprabit-write-test");
            if let Err(e) = std::fs::write(&probe, b"ok") {
                problems.push(format!("LampRabit can't write there: {e}"));
            }
            let _ = std::fs::remove_file(probe);
        }
        Err(e) => problems.push(format!("LampRabit can't create that folder: {e}")),
    }

    let disks = sysinfo::Disks::new_with_refreshed_list();
    let (from, to) = (disk_of(&disks, &cur), disk_of(&disks, &tgt));
    match to {
        Some(disk) => {
            let fs = disk.file_system().to_string_lossy().to_string();
            let lower = fs.to_lowercase();
            if ["vfat", "fat", "fat32", "exfat", "msdos"].contains(&lower.as_str()) {
                problems.push(format!(
                    "That drive uses {fs}, which has no file permissions or links: databases and several runtimes don't work there. Use an NTFS, APFS or ext4 drive."
                ));
            }
            if ["nfs", "nfs4", "cifs", "smb", "smb2", "smb3", "smbfs", "afpfs", "webdav", "fuse.sshfs"].contains(&lower.as_str()) {
                problems.push("That's a network drive: databases on it are slow and can corrupt when the connection drops. Use a local drive.".into());
            }
            out.same_drive = from.is_some_and(|f| f.mount_point() == disk.mount_point());
            out.free = Some(disk.available_space());
            if !out.same_drive && disk.available_space() < size.saturating_add(size / 10) {
                problems.push(format!(
                    "That drive has {} free and the folder needs {} (plus room to grow). Free up space or choose another drive.",
                    human(disk.available_space()),
                    human(size)
                ));
            }
            out.file_system = Some(fs);
        }
        // Windows lists local drives only, so a path on none of them is a network share.
        None if cfg!(windows) => {
            problems.push("That looks like a network drive or share: databases on it are slow and can corrupt. Use a local drive.".into())
        }
        None => {}
    }
    #[cfg(target_os = "linux")]
    if mount_options(&tgt).is_some_and(|o| o.split(',').any(|f| f == "noexec")) {
        problems.push("That drive is mounted noexec, so LampRabit's programs couldn't run from it. Remount it without noexec, or choose another drive.".into());
    }
    if !existed {
        let _ = std::fs::remove_dir(target);
    }
    out.problems = problems;
    out
}

pub fn human(bytes: u64) -> String {
    let gb = bytes as f64 / 1024.0 / 1024.0 / 1024.0;
    if gb >= 1.0 { format!("{gb:.1} GB") } else { format!("{} MB", bytes / 1024 / 1024) }
}

/// Processes that keep the folder busy (running a program from it, or a terminal inside
/// it), other than LampRabit itself.
pub fn holders(root: &Path) -> Vec<Holder> {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    let root = dunce_canonical(root);
    let mut system = System::new();
    system.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_exe(UpdateKind::Always).with_cwd(UpdateKind::Always));
    let me = std::process::id();
    let mut out: Vec<Holder> = system
        .processes()
        .iter()
        .filter(|(pid, p)| pid.as_u32() != me && p.thread_kind().is_none())
        .filter_map(|(pid, p)| {
            let inside = |path: Option<&Path>| path.filter(|x| x.starts_with(&root)).map(|x| x.to_string_lossy().into_owned());
            let path = inside(p.exe()).or_else(|| inside(p.cwd()))?;
            Some(Holder { pid: pid.as_u32(), name: p.name().to_string_lossy().into_owned(), path })
        })
        .collect();
    out.sort_by_key(|h| h.pid);
    out
}

/// Ends a process that holds the folder; only processes `holders` lists can be ended.
pub fn stop_holder(root: &Path, pid: u32) -> Result<()> {
    if !holders(root).iter().any(|h| h.pid == pid) {
        return Err(Error::msg("That process no longer uses the LampRabit folder."));
    }
    let mut system = sysinfo::System::new();
    let target = sysinfo::Pid::from_u32(pid);
    system.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[target]), true);
    match system.process(target) {
        Some(p) if p.kill() => Ok(()),
        Some(_) => Err(Error::msg(format!("Couldn't stop process {pid}. Close it yourself and try again."))),
        None => Ok(()),
    }
}

fn copy_tree(app: &AppHandle, from: &Path, to: &Path, done: &mut u64, total: u64) -> std::io::Result<()> {
    let meta = std::fs::symlink_metadata(from)?;
    if meta.is_dir() {
        std::fs::create_dir_all(to)?;
        for entry in std::fs::read_dir(from)?.flatten() {
            copy_tree(app, &entry.path(), &to.join(entry.file_name()), done, total)?;
        }
    } else if meta.file_type().is_symlink() {
        let link = std::fs::read_link(from)?;
        #[cfg(unix)]
        std::os::unix::fs::symlink(link, to)?;
        #[cfg(windows)]
        {
            let _ = link;
            std::fs::copy(from, to)?;
        }
    } else {
        std::fs::copy(from, to)?;
        *done += meta.len();
        // A progress event per megabyte or so, not per file.
        if meta.len() > 1024 * 1024 || *done % (8 * 1024 * 1024) < meta.len() {
            let _ = app.emit("move-progress", Progress { done: *done, total, current: from.to_string_lossy().into_owned() });
        }
    }
    Ok(())
}

fn in_use(e: &std::io::Error) -> bool {
    e.kind() == std::io::ErrorKind::PermissionDenied || e.raw_os_error() == Some(32) || e.raw_os_error() == Some(5)
}

/// Moves every LampRabit folder from `from` to `to`. All or nothing: on a failure, what
/// was already moved goes back (or the partial copy is removed).
pub fn move_root(app: &AppHandle, from: &Path, to: &Path, same_drive: bool) -> Result<()> {
    std::fs::create_dir_all(to)?;
    let present: Vec<&str> = folders().into_iter().filter(|n| from.join(n).exists()).collect();
    let total: u64 = present.iter().map(|n| size_of(&from.join(n))).sum();
    let hint = |name: &str, e: &std::io::Error| {
        if in_use(e) {
            Error::msg(format!("{name} is in use by a program, so nothing was moved. Check \"Programs using the folder\", stop them, and try again."))
        } else {
            Error::msg(format!("Moving {name} failed, so nothing was moved: {e}"))
        }
    };
    if same_drive {
        let mut moved = Vec::new();
        for name in &present {
            let _ = app.emit("move-progress", Progress { done: 0, total, current: (*name).into() });
            if let Err(e) = std::fs::rename(from.join(name), to.join(name)) {
                for back in moved.iter().rev() {
                    let _ = std::fs::rename(to.join(back), from.join(back));
                }
                return Err(hint(name, &e));
            }
            moved.push(*name);
        }
    } else {
        let mut done = 0;
        for name in &present {
            if let Err(e) = copy_tree(app, &from.join(name), &to.join(name), &mut done, total) {
                for copied in &present {
                    let _ = std::fs::remove_dir_all(to.join(copied));
                }
                return Err(hint(name, &e));
            }
        }
        // The copy is complete before anything is deleted.
        let copied: u64 = present.iter().map(|n| size_of(&to.join(n))).sum();
        if copied < total {
            for name in &present {
                let _ = std::fs::remove_dir_all(to.join(name));
            }
            return Err(Error::msg("The copy came out smaller than the original, so nothing was moved. Check the target drive and try again."));
        }
        for name in &present {
            // Left over only if something reopened it meanwhile; the copy is already in place.
            let _ = std::fs::remove_dir_all(from.join(name));
        }
    }
    let _ = app.emit("move-progress", Progress { done: total, total, current: String::new() });
    // The old folder goes too, when nothing else was in it.
    if std::fs::read_dir(from).is_ok_and(|mut d| d.next().is_none()) {
        let _ = std::fs::remove_dir(from);
    }
    Ok(())
}

/// Python virtual environments record where their Python lives (pyvenv.cfg, and on Linux
/// and macOS symlinks and script shebangs); after a move those point at the old folder.
/// pgAdmin's and the projects' (.venv, venv) are pointed at the new one.
pub fn repair_venvs(old: &Path, new: &Path) {
    let swap = |text: &str| {
        let (o, n) = (old.to_string_lossy(), new.to_string_lossy());
        text.replace(o.as_ref(), n.as_ref()).replace(&o.replace('\\', "/"), &n.replace('\\', "/"))
    };
    let mut venvs = Vec::new();
    for base in [new.join("bin").join("pgadmin"), new.join("www")] {
        for entry in std::fs::read_dir(&base).into_iter().flatten().flatten() {
            for name in ["venv", ".venv"] {
                let venv = entry.path().join(name);
                if venv.join("pyvenv.cfg").is_file() {
                    venvs.push(venv);
                }
            }
        }
    }
    for venv in venvs {
        let cfg = venv.join("pyvenv.cfg");
        if let Ok(text) = std::fs::read_to_string(&cfg) {
            let _ = std::fs::write(&cfg, swap(&text));
        }
        #[cfg(unix)]
        for entry in std::fs::read_dir(venv.join("bin")).into_iter().flatten().flatten() {
            let path = entry.path();
            if let Ok(link) = std::fs::read_link(&path) {
                if link.starts_with(old) {
                    let _ = std::fs::remove_file(&path);
                    let _ = std::os::unix::fs::symlink(new.join(link.strip_prefix(old).unwrap_or(&link)), &path);
                }
            } else if let Ok(text) = std::fs::read_to_string(&path) {
                if text.starts_with("#!") && text.contains(old.to_string_lossy().as_ref()) {
                    let _ = std::fs::write(&path, swap(&text));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_inside_or_around_the_folder_are_refused() {
        let base = std::env::temp_dir().join(format!("lr-move-{}", std::process::id()));
        let current = base.join("current");
        std::fs::create_dir_all(current.join("www").join("shop")).unwrap();
        std::fs::write(current.join("www").join("shop").join("index.php"), "<?php").unwrap();
        assert!(!check(&current, &current.join("inner")).problems.is_empty(), "inside");
        assert!(!check(&current, &base).problems.is_empty(), "around");
        let taken = base.join("taken");
        std::fs::create_dir_all(taken.join("www").join("x")).unwrap();
        assert!(check(&current, &taken).problems.iter().any(|p| p.contains("www")), "clashing folders");
        let fresh = check(&current, &base.join("fresh"));
        assert_eq!(fresh.size, 5);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn venvs_point_at_the_new_folder() {
        let base = std::env::temp_dir().join(format!("lr-venv-{}", std::process::id()));
        let (old, new) = (base.join("old"), base.join("new"));
        let venv = new.join("www").join("blog").join(".venv");
        std::fs::create_dir_all(&venv).unwrap();
        let home = old.join("bin").join("python").join("3.13.1");
        std::fs::write(venv.join("pyvenv.cfg"), format!("home = {}\n", home.display())).unwrap();
        repair_venvs(&old, &new);
        let text = std::fs::read_to_string(venv.join("pyvenv.cfg")).unwrap();
        assert!(text.contains(&new.join("bin").join("python").join("3.13.1").display().to_string()), "{text}");
        let _ = std::fs::remove_dir_all(&base);
    }
}

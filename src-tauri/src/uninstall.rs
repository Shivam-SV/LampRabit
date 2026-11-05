//! Removing LampRabit from a computer: what it set up outside its folder (hosts entries,
//! the trusted CA, PATH, saved tokens) and its own folders. Projects (`www/`) and databases
//! (`data/`) are kept. Used by Settings and by the Windows uninstaller
//! (`LampRabit.exe --uninstall-cleanup`, see windows/hooks.nsh).

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::{config::Settings, layout::Layout, services, shims, sites, ssl, tunnels};

/// Kept: the user's projects, databases, database backups and own project templates.
pub const KEPT: [&str; 4] = ["www", "data", "backups", "templates"];
/// Folders LampRabit creates under the environment root. Only these are deleted, so a root
/// pointed at a folder that holds other things (a home folder) loses nothing else.
pub const OWNED: [&str; 6] = ["bin", "etc", "logs", "tmp", "ssl", "shims"];

/// The app's identifier from tauri.conf.json, for finding its folders without a running app.
const IDENTIFIER: &str = "dev.lamprabit.desktop";
/// The identifier pre-release builds used; their settings may still be around.
const OLD_IDENTIFIER: &str = "dev.lamprabit.app";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    /// Folders that will be deleted (only those that exist).
    pub remove: Vec<PathBuf>,
    /// Folders that stay.
    pub keep: Vec<PathBuf>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub kept: Vec<PathBuf>,
    /// What couldn't be undone, each with what to do about it.
    pub problems: Vec<String>,
}

fn existing(root: &Path, names: &[&str]) -> Vec<PathBuf> {
    names.iter().map(|n| root.join(n)).filter(|p| p.exists()).collect()
}

fn settings_dirs(config_dir: &Path) -> Vec<PathBuf> {
    [config_dir.to_path_buf(), config_dir.with_file_name(OLD_IDENTIFIER)].into_iter().filter(|p| p.exists()).collect()
}

pub fn plan(layout: &Layout, config_dir: &Path) -> Plan {
    let mut remove = existing(layout.root(), &OWNED);
    remove.extend(settings_dirs(config_dir));
    Plan { remove, keep: existing(layout.root(), &KEPT) }
}

/// Undoes everything LampRabit set up. Services must already be stopped. Each step runs
/// even when an earlier one fails, so as much as possible is cleaned up.
pub async fn run(layout: &Layout, config_dir: &Path) -> Report {
    // Outside changes first: the hosts update stages its file in tmp/, deleted below.
    let mut problems = remove_system_changes(layout).await;
    if let Err(e) = shims::set_on_path(layout, false) {
        problems.push(format!("LampRabit's commands couldn't be taken off your PATH: {e}. Remove {} from it by hand.", shims::dir(layout).display()));
    }
    let _ = tunnels::set_ngrok_token(None);
    problems.extend(remove_folders(layout, config_dir));
    Report { kept: existing(layout.root(), &KEPT), problems }
}

fn remove_folders(layout: &Layout, config_dir: &Path) -> Vec<String> {
    let mut problems = Vec::new();
    for dir in existing(layout.root(), &OWNED).into_iter().chain(settings_dirs(config_dir)) {
        if let Err(e) = std::fs::remove_dir_all(&dir) {
            problems.push(format!("Couldn't delete {}: {e}. Close programs using it and delete it by hand.", dir.display()));
        }
    }
    // Only succeeds when nothing is left, i.e. there were no projects or databases.
    let _ = std::fs::remove_dir(layout.root());
    problems
}

/// Hosts entries, the trusted CA and (Linux) the low-ports setting. Linux does all of it
/// in one administrator prompt; Windows and macOS show their own prompt per change, as
/// when they were made.
async fn remove_system_changes(layout: &Layout) -> Vec<String> {
    let mut problems = Vec::new();
    let has_hosts = sites::hosts_has_block();
    let trusted = ssl::ca_trusted(layout).await;

    if cfg!(target_os = "linux") {
        ssl::untrust_in_browsers().await;
        let quote = |p: &Path| format!("'{}'", p.to_string_lossy().replace('\'', r"'\''"));
        // (command, what it removes) for the one administrator prompt.
        let mut steps: Vec<(String, String)> = Vec::new();
        let staged = std::env::temp_dir().join(format!("lamprabit-hosts-{}", std::process::id()));
        if has_hosts {
            match std::fs::read(sites::hosts_file()) {
                Ok(current) if std::fs::write(&staged, sites::hosts_without_block(&current)).is_ok() => {
                    steps.push((format!("cp {} /etc/hosts", quote(&staged)), "the hosts entries".into()));
                }
                _ => problems.push("Couldn't read /etc/hosts; delete the lines between \"# lampRabit begin\" and \"# lampRabit end\" by hand.".into()),
            }
        }
        if let Some((copy, update)) = ssl::linux_trusted_copy().filter(|(copy, _)| copy.exists()) {
            steps.push((format!("rm -f {} && {update}", quote(&copy)), "the trusted certificate".into()));
        }
        let sysctl = Path::new("/etc/sysctl.d/50-lamprabit.conf");
        if sysctl.exists() {
            steps.push((format!("rm -f {}", quote(sysctl)), sysctl.display().to_string()));
        }
        if !steps.is_empty() {
            let script: Vec<&str> = steps.iter().map(|(cmd, _)| cmd.as_str()).collect();
            let ok = tokio::process::Command::new("pkexec")
                .args(["sh", "-c", &script.join("; ")])
                .status()
                .await
                .is_ok_and(|s| s.success());
            if !ok {
                let what: Vec<&str> = steps.iter().map(|(_, what)| what.as_str()).collect();
                let what = match what.as_slice() {
                    [one] => one.to_string(),
                    [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
                    [] => String::new(),
                };
                problems.push(format!(
                    "The administrator prompt was cancelled or failed, so {what} {} left. Remove {} by hand, or run this again.",
                    if steps.len() == 1 { "was" } else { "were" },
                    if steps.len() == 1 { "it" } else { "them" },
                ));
            }
        }
        let _ = std::fs::remove_file(&staged);
        return problems;
    }

    if has_hosts {
        if let Err(e) = sites::update_hosts(layout, &[]).await {
            problems.push(format!("{e} Delete the lines between \"# lampRabit begin\" and \"# lampRabit end\" in {} by hand.", sites::hosts_file().display()));
        }
    }
    if trusted {
        if let Err(e) = ssl::untrust_ca(layout).await {
            problems.push(e.to_string());
        }
    }
    problems
}

/// `LampRabit --uninstall-cleanup`, run by the Windows uninstaller once the app is closed.
/// Without a running app, services left behind are stopped first and the WebView's data
/// can go too.
pub fn cli() {
    let Some(config_dir) = dirs::config_dir().map(|d| d.join(IDENTIFIER)) else { return };
    let (settings, _) = Settings::load_or_recover(&config_dir.join("settings.toml"));
    let layout = Layout::new(&settings.root_dir);
    services::reap_orphans(&layout);
    tauri::async_runtime::block_on(run(&layout, &config_dir));
    if let Some(local) = dirs::data_local_dir().map(|d| d.join(IDENTIFIER)) {
        let _ = std::fs::remove_dir_all(local);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_projects_databases_and_anything_not_ours() {
        let root = std::env::temp_dir().join(format!("lamprabit-uninstall-test-{}", std::process::id()));
        let config = root.with_extension("config");
        for dir in ["bin/php/8.4.27", "etc/apache", "logs", "tmp", "ssl", "shims", "www/shop", "data/mariadb", "Documents"] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
        }
        std::fs::write(root.join("notes.txt"), "mine").unwrap();
        std::fs::create_dir_all(&config).unwrap();
        let layout = Layout::new(&root);

        let plan = plan(&layout, &config);
        assert_eq!(plan.keep, vec![root.join("www"), root.join("data")]);
        assert!(plan.remove.contains(&config));

        assert!(remove_folders(&layout, &config).is_empty());
        for gone in OWNED {
            assert!(!root.join(gone).exists(), "{gone} should be deleted");
        }
        for kept in ["www/shop", "data/mariadb", "Documents", "notes.txt"] {
            assert!(root.join(kept).exists(), "{kept} should be kept");
        }
        assert!(!config.exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn removes_an_empty_root() {
        let root = std::env::temp_dir().join(format!("lamprabit-uninstall-empty-{}", std::process::id()));
        std::fs::create_dir_all(root.join("bin")).unwrap();
        let layout = Layout::new(&root);
        assert!(remove_folders(&layout, &root.with_extension("none")).is_empty());
        assert!(!root.exists());
    }
}

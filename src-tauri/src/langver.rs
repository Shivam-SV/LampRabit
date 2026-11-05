//! Per-project language versions, from the files each ecosystem already uses:
//! `.python-version` (pyenv), `.java-version` (jenv) or `.sdkmanrc`, `go.mod`'s
//! `toolchain`/`go` line, and `rust-toolchain.toml`/`rust-toolchain`. The shims run the
//! matching installed version inside the project's folder; rustup reads Rust's file itself.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::layout::Layout;

/// The runtimes whose projects can ask for a version.
pub const LANGUAGES: [&str; 4] = ["python", "java", "go", "rust"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    /// What the project asks for ("3.12", "21", "1.27.3").
    pub version: String,
    /// The file that says so.
    pub file: &'static str,
}

fn first_line(path: &Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()?
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_string)
}

/// The version a project folder asks for, for one language.
pub fn request(dir: &Path, runtime: &str) -> Option<Request> {
    let found = |version: String, file: &'static str| Some(Request { version, file });
    match runtime {
        "python" => found(first_line(&dir.join(".python-version"))?, ".python-version"),
        "java" => {
            if let Some(v) = first_line(&dir.join(".java-version")) {
                return found(v, ".java-version");
            }
            // java=21.0.2-tem
            let rc = std::fs::read_to_string(dir.join(".sdkmanrc")).ok()?;
            let v = rc.lines().find_map(|l| l.trim().strip_prefix("java="))?;
            found(v.split('-').next().unwrap_or(v).to_string(), ".sdkmanrc")
        }
        "go" => {
            let gomod = std::fs::read_to_string(dir.join("go.mod")).ok()?;
            let line = |key: &str| gomod.lines().find_map(|l| l.trim().strip_prefix(key).map(|v| v.trim().to_string()));
            let v = line("toolchain go").or_else(|| line("go "))?;
            found(v, "go.mod")
        }
        "rust" => {
            if let Ok(toml) = std::fs::read_to_string(dir.join("rust-toolchain.toml")) {
                let v = toml.lines().find_map(|l| {
                    let (k, v) = l.split_once('=')?;
                    (k.trim() == "channel").then(|| v.trim().trim_matches('"').to_string())
                })?;
                return found(v, "rust-toolchain.toml");
            }
            found(first_line(&dir.join("rust-toolchain"))?, "rust-toolchain")
        }
        _ => None,
    }
}

fn parts(v: &str) -> Vec<u32> {
    v.split(|c: char| !c.is_ascii_digit()).filter_map(|p| p.parse().ok()).collect()
}

/// The installed version that satisfies a request: the exact one, else the newest with
/// the same major.minor, else (a request like "21") the newest with the same major.
pub fn matching(installed: &[String], requested: &str) -> Option<String> {
    let want = parts(requested);
    if want.is_empty() {
        return None;
    }
    if let Some(exact) = installed.iter().find(|v| parts(v) == want) {
        return Some(exact.clone());
    }
    let depth = if want.len() >= 2 { 2 } else { 1 };
    installed.iter().filter(|v| parts(v).get(..depth) == want.get(..depth)).max_by_key(|v| parts(v)).cloned()
}

/// Every project in www/ that asks for a version of `runtime`.
pub fn requests(layout: &Layout, runtime: &str) -> Vec<(PathBuf, Request)> {
    std::fs::read_dir(layout.www())
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| Some((e.path(), request(&e.path(), runtime)?)))
        .collect()
}

/// Everything the projects ask for, to notice when a version file changes.
pub fn signature(layout: &Layout) -> String {
    LANGUAGES
        .iter()
        .flat_map(|r| requests(layout, r).into_iter().map(move |(dir, req)| format!("{r}:{}:{}", dir.display(), req.version)))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Writes (or, with None, removes) the version file LampRabit can manage for a project:
/// `.python-version` and `.java-version`. Go and Rust keep theirs in go.mod and
/// rust-toolchain.toml, which are part of the project's own configuration.
pub fn set(dir: &Path, runtime: &str, version: Option<&str>) -> crate::error::Result<()> {
    let file = match runtime {
        "python" => ".python-version",
        "java" => ".java-version",
        "go" => return Err(crate::error::Error::msg("Go projects pick their version in go.mod (the go and toolchain lines).")),
        "rust" => return Err(crate::error::Error::msg("Rust projects pick their version in rust-toolchain.toml; rustup installs it on first use.")),
        _ => return Err(crate::error::Error::msg("This project's language has no version to choose.")),
    };
    match version {
        Some(v) => std::fs::write(dir.join(file), format!("{v}\n"))?,
        None => {
            let _ = std::fs::remove_file(dir.join(file));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_match_the_newest_fitting_install() {
        let installed = vec!["3.12.4".to_string(), "3.12.9".to_string(), "3.13.1".to_string()];
        assert_eq!(matching(&installed, "3.12").as_deref(), Some("3.12.9"));
        assert_eq!(matching(&installed, "3.12.4").as_deref(), Some("3.12.4"));
        assert_eq!(matching(&installed, "3.12.5").as_deref(), Some("3.12.9"), "another patch of the same line");
        assert_eq!(matching(&installed, "3.11"), None);
        let java = vec!["17.0.20".to_string(), "21.0.13".to_string()];
        assert_eq!(matching(&java, "21").as_deref(), Some("21.0.13"));
    }

    #[test]
    fn version_files_are_read() {
        let dir = std::env::temp_dir().join(format!("lr-langver-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(".python-version"), "# pyenv\n3.12\n").unwrap();
        std::fs::write(dir.join(".sdkmanrc"), "java=21.0.2-tem\n").unwrap();
        std::fs::write(dir.join("go.mod"), "module shop\n\ngo 1.26\n\ntoolchain go1.27.3\n").unwrap();
        std::fs::write(dir.join("rust-toolchain.toml"), "[toolchain]\nchannel = \"1.80.0\"\n").unwrap();
        assert_eq!(request(&dir, "python").unwrap().version, "3.12");
        assert_eq!(request(&dir, "java").unwrap(), Request { version: "21.0.2".into(), file: ".sdkmanrc" });
        assert_eq!(request(&dir, "go").unwrap().version, "1.27.3", "toolchain wins over the go line");
        assert_eq!(request(&dir, "rust").unwrap().version, "1.80.0");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

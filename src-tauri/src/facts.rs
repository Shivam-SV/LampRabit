//! Details about a project for its Overview on the Sites page: what it is (package name,
//! version, framework version), what it needs (PHP and Node versions), whether its
//! dependencies are installed, a few settings from its .env, and its git state. Read from
//! small files plus two quick git commands, so it's cheap to load when a project is opened.

use std::{path::Path, process::Stdio, time::Duration};

use serde::Serialize;
use serde_json::Value;

use crate::procgroup;

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Facts {
    /// From composer.json or package.json.
    pub package: Option<String>,
    pub description: Option<String>,
    pub version: Option<String>,
    /// The installed version of the framework (or the declared range when not installed).
    pub framework_version: Option<String>,
    /// composer.json's `require.php`.
    pub php_requirement: Option<String>,
    /// package.json's `engines.node`.
    pub node_requirement: Option<String>,
    pub dependencies: Vec<Dependencies>,
    /// The .env file that was read, and the settings worth seeing at a glance (never keys or passwords).
    pub env_file: Option<String>,
    pub env: Vec<(String, String)>,
    pub git: Option<Git>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dependencies {
    /// "Composer" or "npm" (the Node package manager in use).
    pub manager: String,
    pub count: usize,
    pub dev_count: usize,
    /// vendor/ or node_modules/ exists.
    pub installed: bool,
    /// Installed, but the lock file (or manifest) changed since: a pull or a branch switch.
    pub outdated: bool,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Git {
    /// origin's URL, without any credentials in it.
    pub remote: Option<String>,
    /// Short hash, subject and Unix time (seconds) of the last commit.
    pub commit: Option<(String, String, u64)>,
    /// Files with uncommitted changes; None when git isn't installed.
    pub changes: Option<usize>,
}

/// .env settings shown on the Overview. Secrets (APP_KEY, passwords, tokens) are left out on purpose.
pub const ENV_KEYS: [&str; 10] = ["APP_ENV", "APP_DEBUG", "APP_URL", "DB_CONNECTION", "DB_HOST", "DB_PORT", "MAIL_HOST", "CACHE_STORE", "QUEUE_CONNECTION", "NODE_ENV"];

/// The package that carries each framework's version.
const PHP_PACKAGES: [(&str, &[&str]); 10] = [
    ("Laravel", &["laravel/framework"]),
    ("Statamic", &["statamic/cms"]),
    ("Symfony", &["symfony/framework-bundle"]),
    ("Drupal", &["drupal/core", "drupal/core-recommended"]),
    ("CodeIgniter", &["codeigniter4/framework"]),
    ("CakePHP", &["cakephp/cakephp"]),
    ("Yii", &["yiisoft/yii2"]),
    ("Slim", &["slim/slim"]),
    ("Magento", &["magento/product-community-edition", "mage-os/product-community-edition"]),
    ("PHP", &[]),
];
const NODE_PACKAGES: [(&str, &str); 8] = [
    ("Next.js", "next"),
    ("Nuxt", "nuxt"),
    ("Astro", "astro"),
    ("SvelteKit", "@sveltejs/kit"),
    ("NestJS", "@nestjs/core"),
    ("Remix", "@remix-run/dev"),
    ("Vite", "vite"),
    ("Express", "express"),
];

/// Whether the first of `sources` that exists changed after the first of `marks` that
/// exists (a lock file newer than the install it describes). A few seconds of slack keep a
/// fresh install, which writes both, from counting.
fn newer(dir: &Path, sources: &[&str], marks: &[&str]) -> bool {
    let time = |names: &[&str]| names.iter().find_map(|n| std::fs::metadata(dir.join(n)).and_then(|m| m.modified()).ok());
    match (time(sources), time(marks)) {
        (Some(source), Some(mark)) => source.duration_since(mark).is_ok_and(|d| d > Duration::from_secs(5)),
        _ => false,
    }
}

fn json(path: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn text(v: &Value) -> Option<String> {
    v.as_str().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
}

pub async fn read(dir: &Path, framework: Option<&str>, package_manager: &str) -> Facts {
    let composer = json(&dir.join("composer.json"));
    let package = json(&dir.join("package.json"));
    let mut facts = Facts::default();

    // Identity: package.json for JavaScript projects, composer.json otherwise.
    for source in [composer.as_ref(), package.as_ref()].into_iter().flatten() {
        facts.package = facts.package.or_else(|| text(&source["name"]));
        facts.description = facts.description.or_else(|| text(&source["description"]));
        facts.version = facts.version.or_else(|| text(&source["version"]));
    }

    if let Some(c) = &composer {
        facts.php_requirement = text(&c["require"]["php"]);
        let count = |key: &str| c[key].as_object().map_or(0, |m| m.keys().filter(|k| *k != "php" && !k.starts_with("ext-")).count());
        let installed = dir.join("vendor/autoload.php").is_file();
        let outdated = installed && newer(dir, &["composer.lock", "composer.json"], &["vendor/composer/installed.json"]);
        facts.dependencies.push(Dependencies { manager: "Composer".into(), count: count("require"), dev_count: count("require-dev"), installed, outdated });
    }
    if let Some(p) = &package {
        facts.node_requirement = text(&p["engines"]["node"]);
        let count = |key: &str| p[key].as_object().map_or(0, |m| m.len());
        let installed = dir.join("node_modules").is_dir();
        // Each package manager leaves its own note of what it installed.
        let marks = ["node_modules/.package-lock.json", "node_modules/.modules.yaml", "node_modules/.yarn-state.yml", "node_modules/.yarn-integrity", "node_modules"];
        let outdated = installed && newer(dir, &["package-lock.json", "pnpm-lock.yaml", "yarn.lock", "bun.lock", "bun.lockb", "package.json"], &marks);
        facts.dependencies.push(Dependencies { manager: package_manager.into(), count: count("dependencies"), dev_count: count("devDependencies"), installed, outdated });
    }

    facts.framework_version = framework.and_then(|f| framework_version(dir, f, composer.as_ref(), package.as_ref()));

    if let Some(file) = [".env.local", ".env"].into_iter().find(|f| dir.join(f).is_file()) {
        let content = std::fs::read_to_string(dir.join(file)).unwrap_or_default();
        facts.env = ENV_KEYS
            .iter()
            .filter_map(|key| {
                let value = content.lines().find_map(|l| l.trim().strip_prefix(&format!("{key}=")))?;
                let value = value.trim().trim_matches(['"', '\'']);
                (!value.is_empty()).then(|| (key.to_string(), value.to_string()))
            })
            .collect();
        facts.env_file = Some(file.to_string());
    }

    if dir.join(".git").exists() {
        facts.git = Some(git(dir).await);
    }
    facts
}

/// The framework's installed version: composer.lock, node_modules, or WordPress's own
/// version file; the declared range when it isn't installed.
fn framework_version(dir: &Path, framework: &str, composer: Option<&Value>, package: Option<&Value>) -> Option<String> {
    if framework == "WordPress" {
        let source = std::fs::read_to_string(dir.join("wp-includes/version.php")).ok()?;
        let line = source.lines().find(|l| l.trim_start().starts_with("$wp_version"))?;
        return line.split(['\'', '"']).nth(1).map(str::to_string);
    }
    if let Some((_, names)) = PHP_PACKAGES.iter().find(|(f, _)| *f == framework) {
        let lock = json(&dir.join("composer.lock"));
        for name in *names {
            let installed = lock.as_ref().and_then(|l| {
                l["packages"].as_array()?.iter().find(|p| p["name"] == *name).and_then(|p| text(&p["version"]))
            });
            if let Some(v) = installed.or_else(|| composer.and_then(|c| text(&c["require"][*name]))) {
                return Some(v.trim_start_matches('v').to_string());
            }
        }
        return None;
    }
    let (_, name) = NODE_PACKAGES.iter().find(|(f, _)| *f == framework)?;
    json(&dir.join("node_modules").join(name).join("package.json"))
        .and_then(|p| text(&p["version"]))
        .or_else(|| package.and_then(|p| text(&p["dependencies"][*name]).or_else(|| text(&p["devDependencies"][*name]))))
}

async fn git(dir: &Path) -> Git {
    let config = std::fs::read_to_string(dir.join(".git/config")).unwrap_or_default();
    let remote = config
        .split("[remote \"origin\"]")
        .nth(1)
        .and_then(|section| section.lines().find_map(|l| l.trim().strip_prefix("url = ")))
        .map(|url| without_credentials(url.trim()));
    let commit = run_git(dir, &["log", "-1", "--format=%h%x1f%s%x1f%ct"]).await.and_then(|out| {
        let mut parts = out.trim().splitn(3, '\u{1f}');
        Some((parts.next()?.to_string(), parts.next()?.to_string(), parts.next()?.parse().ok()?))
    });
    let changes = run_git(dir, &["status", "--porcelain"]).await.map(|out| out.lines().filter(|l| !l.trim().is_empty()).count());
    Git { remote, commit, changes }
}

/// `https://user:token@host/x.git` → `https://host/x.git`, so a token never reaches the UI.
fn without_credentials(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else { return url.to_string() };
    let (authority, path) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
    match authority.rsplit_once('@') {
        Some((_, host)) => format!("{scheme}://{host}{path}"),
        None => url.to_string(),
    }
}

async fn run_git(dir: &Path, args: &[&str]) -> Option<String> {
    let mut cmd = tokio::process::Command::new("git");
    cmd.args(args).current_dir(dir).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).kill_on_drop(true);
    procgroup::prepare(&mut cmd);
    let out = tokio::time::timeout(Duration::from_secs(5), cmd.output()).await.ok()?.ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_urls_lose_their_credentials() {
        assert_eq!(without_credentials("https://me:ghp_secret@github.com/a/b.git"), "https://github.com/a/b.git");
        assert_eq!(without_credentials("https://github.com/a/b.git"), "https://github.com/a/b.git");
        assert_eq!(without_credentials("git@github.com:a/b.git"), "git@github.com:a/b.git");
        assert_eq!(without_credentials("ssh://git@host/x@y.git"), "ssh://host/x@y.git");
    }

    #[test]
    fn reads_versions_and_env_without_secrets() {
        let dir = std::env::temp_dir().join(format!("lamprabit-facts-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("composer.json"), r#"{"name":"acme/shop","require":{"php":"^8.2","laravel/framework":"^12.0","ext-gd":"*"}}"#).unwrap();
        std::fs::write(dir.join("composer.lock"), r#"{"packages":[{"name":"laravel/framework","version":"v12.3.1"}]}"#).unwrap();
        std::fs::write(dir.join(".env"), "APP_ENV=local\nAPP_KEY=base64:secret\nDB_PASSWORD=hunter2\nAPP_URL=\"https://shop.test\"\n").unwrap();
        let facts = tauri::async_runtime::block_on(read(&dir, Some("Laravel"), "npm"));
        assert_eq!(facts.package.as_deref(), Some("acme/shop"));
        assert_eq!(facts.framework_version.as_deref(), Some("12.3.1"));
        assert_eq!(facts.php_requirement.as_deref(), Some("^8.2"));
        assert_eq!(facts.dependencies[0].count, 1, "php and ext-* aren't packages");
        assert!(!facts.dependencies[0].installed);
        assert_eq!(facts.env, vec![("APP_ENV".to_string(), "local".to_string()), ("APP_URL".to_string(), "https://shop.test".to_string())]);
        let _ = std::fs::remove_dir_all(dir);
    }
}

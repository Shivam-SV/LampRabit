//! `lamprabit.toml`: a project's setup in its own folder, to commit and share. It holds what
//! makes the project run here (PHP version, web server, HTTPS, PHP settings, where the main
//! domain goes, subdomains and processes), never anything personal like Xdebug.
//!
//! It is never applied on its own: processes are commands, and a cloned repository
//! shouldn't run anything until someone looks. The project's Settings offer to apply it,
//! showing everything it sets, through the same commands (and checks) as editing by hand.
//! settings.toml stays the source of truth; the file is an import and export format.

use std::{collections::BTreeMap, path::Path};

use serde::{Deserialize, Serialize};

use crate::{
    config::{DomainTarget, ProjectProcess, Server, SiteSettings},
    error::{Error, Result},
};

pub const FILE: &str = "lamprabit.toml";

/// A subdomain by its first label ("admin" for admin.<project>.test), so the file works
/// whatever the folder is called or the domain ending is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subdomain {
    pub name: String,
    pub target: DomainTarget,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ProjectFile {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub php: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<Server>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssl: Option<bool>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub php_ini: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub main: Option<DomainTarget>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub domains: Vec<Subdomain>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub processes: Vec<ProjectProcess>,
}

impl ProjectFile {
    /// The project's current setup, with its subdomains relative to `parent` (shop.test).
    pub fn from_settings(own: &SiteSettings, parent: &str) -> Self {
        let suffix = format!(".{parent}");
        Self {
            php: own.php.clone(),
            server: own.server,
            ssl: own.ssl,
            php_ini: own.php_ini.clone(),
            main: own.main.clone(),
            domains: own.domains.iter().map(|d| Subdomain { name: d.host.strip_suffix(&suffix).unwrap_or(&d.host).to_string(), target: d.target.clone() }).collect(),
            processes: own.processes.clone(),
        }
    }
}

/// The project's file, when it has one.
pub fn read(dir: &Path) -> Result<Option<ProjectFile>> {
    let path = dir.join(FILE);
    if !path.is_file() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path)?;
    toml::from_str(&text).map(Some).map_err(|e| Error::msg(format!("{FILE} can't be read: {e}")))
}

/// Writes the project's setup to its folder, with a note on what the file is.
pub fn write(dir: &Path, file: &ProjectFile) -> Result<()> {
    let body = toml::to_string_pretty(file)?;
    let text = format!(
        "# LampRabit setup for this project: commit it so others get the same domains,\n# processes and PHP settings. Apply it in LampRabit: Sites → this project → Settings.\n\n{body}"
    );
    std::fs::write(dir.join(FILE), text)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ExtraDomain, RestartPolicy};

    #[test]
    fn round_trips_through_toml() {
        let mut own = SiteSettings { php: Some("8.3".into()), ssl: Some(true), ..Default::default() };
        own.domains.push(ExtraDomain { host: "admin.shop.test".into(), target: DomainTarget::Files { root: "admin/public".into() } });
        own.processes.push(ProjectProcess { name: "queue".into(), command: "php artisan queue:work".into(), restart: RestartPolicy::Always, ..Default::default() });
        let file = ProjectFile::from_settings(&own, "shop.test");
        assert_eq!(file.domains[0].name, "admin");
        let text = toml::to_string_pretty(&file).unwrap();
        let back: ProjectFile = toml::from_str(&text).unwrap();
        assert_eq!(back, file);
        // An empty file is a valid one.
        assert_eq!(toml::from_str::<ProjectFile>("").unwrap(), ProjectFile::default());
    }
}

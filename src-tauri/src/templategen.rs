//! "Save as template": a project's setup written as a template that recreates it, to start
//! the next one the same way or to hand to a teammate. The code comes from its Git remote
//! (a project without one gets an empty folder and a note saying so); then whatever its
//! files show it needs: Composer and Node packages, .env from .env.example, Laravel's key
//! and migrations, the database; and its PHP version, processes, subdomains, dev server
//! and own commands. The result opens in the template editor to be read and adjusted
//! before saving, so nothing here has to guess perfectly.

use std::path::Path;

use crate::{
    config::{DomainTarget, SiteSettings},
    manage::Engine,
};

/// What the generator knows about the project.
pub struct Setup<'a> {
    pub name: &'a str,
    pub dir: &'a Path,
    /// origin's URL, credentials already stripped.
    pub remote: Option<String>,
    /// The PHP version it runs on ("8.3.12"), for PHP projects.
    pub php: Option<String>,
    pub engine: Option<Engine>,
    pub has_database: bool,
    pub own: Option<&'a SiteSettings>,
    /// The parent domain (shop.test), to turn subdomains back into their first label.
    pub parent: String,
}

/// A YAML single-quoted string.
fn q(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

/// "shop" → "shop-setup"; ids are letters, digits, dashes and underscores.
fn id_for(name: &str) -> String {
    let base: String = name.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '-' }).collect();
    format!("{}-setup", base.trim_matches('-'))
}

pub fn generate(s: &Setup) -> String {
    let has = |f: &str| s.dir.join(f).exists();
    let composer = has("composer.json");
    let node = has("package.json");
    let laravel = has("artisan");
    let mut y = String::new();
    y.push_str(&format!("schema: 1\nid: {}\nname: {}\n", id_for(s.name), q(&format!("{} setup", s.name))));
    y.push_str(&format!("description: {}\n", q(&format!("Recreates {}: its code, dependencies, database, processes and domains.", s.name))));
    y.push_str("icon: package\n");

    // What it needs.
    let php_line = s.php.as_deref().map(|v| v.split('.').take(2).collect::<Vec<_>>().join("."));
    if php_line.is_some() || node {
        y.push_str("requires:\n");
        if let Some(line) = &php_line {
            y.push_str(&format!("  php: {}\n", q(line)));
        }
        if node {
            y.push_str("  node: true\n");
        }
    }
    if s.has_database {
        y.push_str("database: optional\n");
        if let Some(engine) = s.engine {
            let id = match engine {
                Engine::Mariadb => "mariadb",
                Engine::Mysql => "mysql",
                Engine::Postgres => "postgres",
                Engine::Sqlite => "sqlite",
            };
            y.push_str(&format!("engines: [{id}]\n"));
        }
    }

    y.push_str("steps:\n");
    if s.has_database {
        y.push_str("  - create_database: {}\n");
    }
    match &s.remote {
        Some(remote) => {
            y.push_str("  - name: Getting the code\n    cwd: www\n    needs: [git]\n");
            y.push_str(&format!("    run: {}\n", q(&format!("git clone {remote} \"{{{{project}}}}\""))));
        }
        None => {
            // A template needs a step; without a remote there's no code to fetch.
            y.push_str("  # This project has no Git remote, so the template can't fetch its code. Add a\n  # `run: git clone <url> \"{{project}}\"` step (with cwd: www) once it has one.\n");
            y.push_str("  - name: Writing a README\n    write:\n      path: README.md\n      content: \"# {{project}}\\n\"\n");
        }
    }
    if composer {
        y.push_str("  - name: Installing PHP packages\n    composer: [install, --no-interaction]\n");
    }
    if has(".env.example") {
        y.push_str("  - name: Creating .env\n    copy: { from: .env.example, to: .env }\n");
    }
    if laravel {
        y.push_str("  - name: Configuring the environment\n    env:\n      values:\n        APP_URL: \"{{url}}\"\n        MAIL_HOST: \"{{mail.host}}\"\n        MAIL_PORT: \"{{mail.port}}\"\n");
        if s.has_database {
            y.push_str("  - name: Connecting the database\n    if: database\n    env:\n      values:\n        DB_CONNECTION: \"{{db.engine}}\"\n        DB_HOST: \"{{db.host}}\"\n        DB_PORT: \"{{db.port}}\"\n        DB_DATABASE: \"{{database}}\"\n        DB_USERNAME: \"{{db.user}}\"\n        DB_PASSWORD: \"{{db.password}}\"\n");
        }
        y.push_str("  - name: Generating the app key\n    php: [artisan, key:generate]\n");
        if s.has_database {
            y.push_str("  - name: Running database migrations\n    if: database\n    php: [artisan, migrate, --force]\n");
        }
    }
    if node {
        y.push_str("  - name: Installing Node packages\n    needs: [node]\n    run: \"{{pm.install}}\"\n");
    }

    let Some(own) = s.own else { return y };
    // How it's served: its dev server (or a PHP project's asset server).
    if let Some(app) = own.app.as_ref().filter(|a| !a.command.trim().is_empty() && (node || !a.serve)) {
        let mode = if app.serve { "dynamic" } else { "static" };
        y.push_str(&format!("serve:\n  mode: {mode}\n  command: {}\n", q(app.command.trim())));
    }
    if !own.commands.is_empty() {
        y.push_str("commands:\n  list:\n");
        for c in &own.commands {
            y.push_str(&format!("    - name: {}\n      run: {}\n", q(&c.name), q(&c.run)));
        }
    }
    if !own.processes.is_empty() {
        y.push_str("processes:\n");
        for p in &own.processes {
            y.push_str(&format!("  - name: {}\n    command: {}\n", p.name, q(&p.command)));
            if !p.cwd.is_empty() {
                y.push_str(&format!("    cwd: {}\n", q(&p.cwd)));
            }
            // Ports are picked afresh: the next project's must not clash with this one's.
            if p.port.is_some() {
                y.push_str("    port: auto\n");
            }
            let restart = match p.restart {
                crate::config::RestartPolicy::Never => "never",
                crate::config::RestartPolicy::OnFailure => "on-failure",
                crate::config::RestartPolicy::Always => "always",
            };
            y.push_str(&format!("    restart: {restart}\n"));
            if !p.with_app {
                y.push_str("    autostart: false\n");
            }
            if !p.env.is_empty() {
                y.push_str("    env:\n");
                for (k, v) in &p.env {
                    y.push_str(&format!("      {k}: {}\n", q(v)));
                }
            }
        }
    }
    if !own.domains.is_empty() {
        y.push_str("domains:\n");
        let suffix = format!(".{}", s.parent);
        for d in &own.domains {
            let name = d.host.strip_suffix(&suffix).unwrap_or(&d.host);
            y.push_str(&format!("  - name: {name}\n"));
            match &d.target {
                DomainTarget::Main => {}
                DomainTarget::Files { root } => y.push_str(&format!("    root: {}\n", q(root))),
                // A fixed port of this machine means little elsewhere; keep it, it's visible.
                DomainTarget::Port { port } => y.push_str(&format!("    port: {port}\n")),
                DomainTarget::Process { name } => y.push_str(&format!("    process: {name}\n")),
            }
        }
    }
    y
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ExtraDomain, ProjectProcess};

    #[test]
    fn a_projects_setup_makes_a_valid_template() {
        let dir = std::env::temp_dir().join(format!("lamprabit-gen-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for f in ["composer.json", "package.json", "artisan", ".env.example"] {
            std::fs::write(dir.join(f), "{}").unwrap();
        }
        let mut own = SiteSettings::default();
        own.processes.push(ProjectProcess { name: "queue".into(), command: "php artisan queue:work".into(), with_app: false, ..Default::default() });
        own.processes.push(ProjectProcess { name: "api".into(), command: "node api.js --port {port}".into(), port: Some(3100), ..Default::default() });
        own.domains.push(ExtraDomain { host: "api.shop.test".into(), target: DomainTarget::Process { name: "api".into() } });
        own.domains.push(ExtraDomain { host: "admin.shop.test".into(), target: DomainTarget::Files { root: "admin's/public".into() } });
        let yaml = generate(&Setup {
            name: "Shop",
            dir: &dir,
            remote: Some("https://github.com/acme/shop.git".into()),
            php: Some("8.3.12".into()),
            engine: Some(Engine::Mariadb),
            has_database: true,
            own: Some(&own),
            parent: "shop.test".into(),
        });
        let t = crate::templates::parse_text(&yaml).unwrap_or_else(|e| panic!("{e}\n{yaml}"));
        assert_eq!(t.id, "shop-setup");
        assert_eq!(t.processes.len(), 2);
        assert_eq!(t.domains[1].root.as_deref(), Some("admin's/public"));
        // Without a remote it still parses.
        let bare = generate(&Setup { name: "x", dir: &dir, remote: None, php: None, engine: None, has_database: false, own: None, parent: "x.test".into() });
        crate::templates::parse_text(&bare).unwrap_or_else(|e| panic!("{e}\n{bare}"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

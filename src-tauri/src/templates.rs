//! Project templates: YAML files describing how to create a project, step by step. The
//! built-in ones ship in the app (src-tauri/templates/); your own go in `<root>/templates/`.
//! docs/templates.md is the reference for the format; projects.rs runs the steps.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{
    error::{Error, Result},
    layout::Layout,
};

/// The format version this app understands (`schema:` in a template).
pub const SCHEMA: u32 = 1;

/// Built-in templates, in the order the wizard shows them.
const BUILT_IN: &[(&str, &str)] = &[
    ("blank.yaml", include_str!("../templates/blank.yaml")),
    ("laravel.yaml", include_str!("../templates/laravel.yaml")),
    ("symfony.yaml", include_str!("../templates/symfony.yaml")),
    ("wordpress.yaml", include_str!("../templates/wordpress.yaml")),
    ("cakephp.yaml", include_str!("../templates/cakephp.yaml")),
    ("codeigniter.yaml", include_str!("../templates/codeigniter.yaml")),
    ("drupal.yaml", include_str!("../templates/drupal.yaml")),
    ("magento.yaml", include_str!("../templates/magento.yaml")),
    ("statamic.yaml", include_str!("../templates/statamic.yaml")),
    ("vite.yaml", include_str!("../templates/vite.yaml")),
    ("nextjs.yaml", include_str!("../templates/nextjs.yaml")),
    ("nuxt.yaml", include_str!("../templates/nuxt.yaml")),
    ("astro.yaml", include_str!("../templates/astro.yaml")),
    ("express.yaml", include_str!("../templates/express.yaml")),
    ("nestjs.yaml", include_str!("../templates/nestjs.yaml")),
    ("django.yaml", include_str!("../templates/django.yaml")),
    ("flask.yaml", include_str!("../templates/flask.yaml")),
    ("fastapi.yaml", include_str!("../templates/fastapi.yaml")),
    ("spring-boot.yaml", include_str!("../templates/spring-boot.yaml")),
    ("java-webapp.yaml", include_str!("../templates/java-webapp.yaml")),
    ("go.yaml", include_str!("../templates/go.yaml")),
    ("gin.yaml", include_str!("../templates/gin.yaml")),
    ("axum.yaml", include_str!("../templates/axum.yaml")),
    ("static.yaml", include_str!("../templates/static.yaml")),
];

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Template {
    pub schema: u32,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// The template's own version, recorded with each project created from it.
    #[serde(default = "one")]
    pub version: u32,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    /// A hint shown in the wizard ("Composer · about a minute").
    #[serde(default)]
    pub duration: Option<String>,
    #[serde(default)]
    pub requires: Requires,
    #[serde(default)]
    pub database: DatabaseUse,
    /// The database servers the template can set the project up for; MariaDB and MySQL
    /// when absent (most PHP apps' configs are written for them).
    #[serde(default = "mysql_family")]
    pub engines: Vec<DbEngine>,
    #[serde(default)]
    pub variables: Vec<Variable>,
    pub steps: Vec<Step>,
    /// Shown when the project is ready (what to do next).
    #[serde(default)]
    pub done: Option<String>,
    /// How the project's domain is served: a dev server LampRabit runs (dynamic) or the
    /// files themselves (static). Without it, the web server serves the files (PHP).
    #[serde(default, alias = "app")]
    pub serve: Option<ServeTemplate>,
    /// What the project's Commands panel offers.
    #[serde(default)]
    pub commands: Option<CommandsTemplate>,
    /// Processes the project gets (Sites → Commands → Keep running): a queue worker, Vite.
    #[serde(default)]
    pub processes: Vec<TemplateProcess>,
    /// Subdomains the project gets (Sites → Domains): admin.<project>.test to a folder,
    /// a port or one of the processes above.
    #[serde(default)]
    pub domains: Vec<TemplateDomain>,
}

/// A process a template's projects get.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateProcess {
    /// Lowercase letters, digits and dashes: "queue", "vite".
    pub name: String,
    /// Placeholders work; `{port}` is the process's port.
    pub command: String,
    /// A folder of the project to run in; the project folder when absent.
    #[serde(default)]
    pub cwd: String,
    /// A port number, or "auto" for a free one picked when the project is created.
    #[serde(default)]
    pub port: Option<String>,
    #[serde(default)]
    pub restart: crate::config::RestartPolicy,
    /// Starts when the project starts; false leaves it to be started by hand.
    #[serde(default = "yes")]
    pub autostart: bool,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Added only when this holds (like a step's `if:`).
    #[serde(default, rename = "if", alias = "when")]
    pub when: Option<String>,
}

/// A subdomain a template's projects get; at most one of `root`, `port` and `process`
/// (none: wherever the main domain goes).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateDomain {
    /// The first label: "admin" makes admin.<project>.test.
    pub name: String,
    /// A folder of the project ("admin/public"); "" for the main domain's folder.
    #[serde(default)]
    pub root: Option<String>,
    #[serde(default)]
    pub port: Option<u16>,
    /// One of the template's processes, which needs a port.
    #[serde(default)]
    pub process: Option<String>,
    #[serde(default, rename = "if", alias = "when")]
    pub when: Option<String>,
}

fn yes() -> bool {
    true
}

impl TemplateDomain {
    pub fn target(&self) -> crate::config::DomainTarget {
        use crate::config::DomainTarget;
        match (&self.root, self.port, &self.process) {
            (Some(root), _, _) => DomainTarget::Files { root: root.clone() },
            (_, Some(port), _) => DomainTarget::Port { port },
            (_, _, Some(name)) => DomainTarget::Process { name: name.clone() },
            _ => DomainTarget::Main,
        }
    }
}

/// A template's commands: where to discover them, and fixed ones of its own.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandsTemplate {
    /// Sources to list commands from; every source the project's files show when absent.
    #[serde(default)]
    pub discover: Option<Vec<crate::commands::Source>>,
    #[serde(default)]
    pub list: Vec<TemplateCommand>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateCommand {
    pub name: String,
    /// Runs in the project folder; placeholders work.
    pub run: String,
    #[serde(default)]
    pub description: Option<String>,
}

fn one() -> u32 {
    1
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requires {
    /// Lowest PHP version ("8.2"), or "any"; absent means the project doesn't use PHP.
    pub php: Option<String>,
    /// PHP extensions that must be loaded (checked with `php -m`).
    #[serde(default)]
    pub extensions: Vec<String>,
    /// A Node.js project: the wizard asks for the Node version and package manager.
    #[serde(default)]
    pub node: bool,
    /// Lowest Python version ("3.10"): LampRabit's Python must be installed.
    #[serde(default)]
    pub python: Option<String>,
    /// Lowest Java version ("17"): LampRabit's Java must be installed.
    #[serde(default)]
    pub java: Option<String>,
    /// Lowest Go version ("1.22"): LampRabit's Go must be installed.
    #[serde(default)]
    pub go: Option<String>,
    /// Lowest Rust version ("1.80"): LampRabit's Rust must be installed.
    #[serde(default)]
    pub rust: Option<String>,
}

/// How a Node project runs once created; saved as its app setup (Sites → Node app).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServeTemplate {
    #[serde(default)]
    pub mode: crate::config::ServeMode,
    /// What starts the dev server; placeholders work, and `{port}` is filled in with the
    /// port LampRabit picks when it starts.
    #[serde(default)]
    pub command: String,
    /// Instead of a command: a script for `node` to run.
    #[serde(default)]
    pub file: String,
    /// With `file`: restart when files change (`node --watch`).
    #[serde(default)]
    pub watch: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DatabaseUse {
    /// No database offered.
    #[default]
    None,
    /// The wizard offers one, on by default when a database server is installed.
    Optional,
    /// The project can't work without one.
    Required,
}

/// A database server a project can be created on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DbEngine {
    Mariadb,
    Mysql,
    Postgres,
    /// A file in the project; nothing to install or start.
    Sqlite,
    /// Only for backups; no template sets a project up on it yet.
    Mongodb,
}

impl DbEngine {
    pub fn id(self) -> &'static str {
        match self {
            Self::Mariadb => "mariadb",
            Self::Mysql => "mysql",
            Self::Postgres => "postgres",
            Self::Sqlite => "sqlite",
            Self::Mongodb => "mongodb",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Mariadb => "MariaDB",
            Self::Mysql => "MySQL",
            Self::Postgres => "PostgreSQL",
            Self::Sqlite => "SQLite",
            Self::Mongodb => "MongoDB",
        }
    }
}

fn mysql_family() -> Vec<DbEngine> {
    vec![DbEngine::Mariadb, DbEngine::Mysql]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Variable {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub help: Option<String>,
    #[serde(rename = "type", default)]
    pub kind: VariableKind,
    /// For `choice` and `multi`: plain values, or `{value, label, if}`.
    #[serde(default)]
    pub options: Vec<VariableOption>,
    /// `bool`: what a yes or a no puts in the steps (`--eslint` / `--no-eslint`);
    /// "true" and "false" when absent.
    #[serde(default)]
    pub on: Option<String>,
    #[serde(default)]
    pub off: Option<String>,
    /// Wraps a non-empty value, e.g. `--add {value}`; an empty one renders as nothing.
    #[serde(default)]
    pub flag: Option<String>,
    /// `multi`: what joins the picked values (default ",").
    #[serde(default)]
    pub separator: Option<String>,
    #[serde(default)]
    pub default: Option<String>,
    /// Shown (and used) only when this holds, like a step's `if:`.
    #[serde(default, rename(deserialize = "if"))]
    pub when: Option<String>,
    /// The wizard won't create the project while it's empty (when shown).
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VariableKind {
    #[default]
    Text,
    Choice,
    Bool,
    /// Several of the options (checkboxes).
    Multi,
}

/// An option of a choice: written as a plain value, or with a label and a condition (shown
/// only while it holds, so options can follow earlier answers).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VariableOption {
    pub value: String,
    pub label: String,
    pub when: Option<String>,
}

impl<'de> Deserialize<'de> for VariableOption {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Full {
            value: String,
            label: Option<String>,
            #[serde(rename = "if")]
            when: Option<String>,
        }
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Form {
            Plain(String),
            Full(Full),
        }
        Ok(match Form::deserialize(d)? {
            Form::Plain(value) => VariableOption { label: value.clone(), value, when: None },
            Form::Full(f) => VariableOption { label: f.label.unwrap_or_else(|| f.value.clone()), value: f.value, when: f.when },
        })
    }
}

impl Variable {
    /// What `{{vars.<id>}}` renders from the value picked in the wizard.
    pub fn render(&self, raw: &str) -> String {
        let value = match self.kind {
            VariableKind::Bool => {
                let on = raw == "true";
                match (on, &self.on, &self.off) {
                    (true, Some(v), _) | (false, _, Some(v)) => v.clone(),
                    (true, None, _) => "true".into(),
                    (false, _, None) => "false".into(),
                }
            }
            VariableKind::Multi => raw.split(',').map(str::trim).filter(|v| !v.is_empty()).collect::<Vec<_>>().join(self.separator.as_deref().unwrap_or(",")),
            _ => raw.to_string(),
        };
        match &self.flag {
            Some(flag) if !value.is_empty() => flag.replace("{value}", &value),
            Some(_) => String::new(),
            None => value,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Step {
    /// Names an optional step in requests; defaults to its position.
    #[serde(default)]
    pub id: Option<String>,
    /// The progress heading ("Running database migrations").
    #[serde(default)]
    pub name: Option<String>,
    /// Runs only when this holds: `database`, `!database`, `vars.kit == react`, `vars.kit != none`.
    #[serde(default, rename = "if")]
    pub when: Option<String>,
    /// The wizard shows a checkbox for it.
    #[serde(default)]
    pub optional: bool,
    /// The checkbox text for an optional step.
    #[serde(default)]
    pub label: Option<String>,
    /// Whether an optional step starts ticked (default yes).
    #[serde(default)]
    pub default: Option<bool>,
    /// Tools the step needs on PATH: `node` (npm, npx), `git`.
    #[serde(default)]
    pub needs: Vec<String>,
    /// Working folder: `www` or a folder inside the project; the project folder by default.
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(flatten)]
    pub action: Action,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// Composer with the project's PHP: `composer: [create-project, laravel/laravel, "{{project}}"]`.
    Composer(Vec<String>),
    /// The project's PHP: `php: [artisan, migrate, --force]`.
    Php(Vec<String>),
    /// A shell command (cmd on Windows, sh elsewhere), with php, composer and mysql on PATH.
    Run(String),
    Download(Download),
    Write(FileContent),
    Append(FileContent),
    Copy(CopyFile),
    /// Sets keys in a .env-style file, adding missing ones.
    Env(EnvFile),
    /// Replaces text in a file; each occurrence gets a freshly rendered value.
    Replace(ReplaceText),
    /// Creates the database chosen in the wizard (skipped when none was).
    CreateDatabase(Empty),
    /// `git init`, and a first commit when Git knows who you are.
    GitInit(GitInit),
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Empty {}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Download {
    pub url: String,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub sha256_url: Option<String>,
    #[serde(default)]
    pub sha1_url: Option<String>,
    /// Unpack a .zip or .tar.gz into the project (else save it as `to`).
    #[serde(default)]
    pub extract: bool,
    /// Take the contents of this folder inside the archive (WordPress: `wordpress`).
    #[serde(default)]
    pub strip: Option<String>,
    #[serde(default)]
    pub to: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileContent {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CopyFile {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvFile {
    #[serde(default = "dot_env")]
    pub file: String,
    pub values: serde_yaml_ng::Mapping,
}

fn dot_env() -> String {
    ".env".into()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplaceText {
    pub file: String,
    pub values: serde_yaml_ng::Mapping,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitInit {
    #[serde(default)]
    pub commit: Option<String>,
}

impl Step {
    pub fn key(&self, index: usize) -> String {
        self.id.clone().unwrap_or_else(|| index.to_string())
    }

    /// The progress heading: the template's, else one describing the action.
    pub fn heading(&self) -> String {
        if let Some(name) = &self.name {
            return name.clone();
        }
        match &self.action {
            Action::Composer(args) => format!("composer {}", args.first().map(String::as_str).unwrap_or_default()),
            Action::Php(args) => format!("php {}", args.join(" ")),
            Action::Run(cmd) => cmd.clone(),
            Action::Download(d) => format!("Downloading {}", d.url.rsplit('/').next().unwrap_or(&d.url)),
            Action::Write(f) => format!("Writing {}", f.path),
            Action::Append(f) => format!("Updating {}", f.path),
            Action::Copy(c) => format!("Copying {} to {}", c.from, c.to),
            Action::Env(e) => format!("Configuring {}", e.file),
            Action::Replace(r) => format!("Updating {}", r.file),
            Action::CreateDatabase(_) => "Creating the database".into(),
            Action::GitInit(_) => "Initialising a Git repository".into(),
        }
    }

    /// The command or URL behind a step, as written in the template.
    pub fn what_runs(&self) -> String {
        match &self.action {
            Action::Run(cmd) => cmd.clone(),
            Action::Download(d) => d.url.clone(),
            _ => self.heading(),
        }
    }

    /// Every string the step renders, for checking placeholders when the template loads.
    fn strings(&self) -> Vec<String> {
        let mut out: Vec<String> = self.when.iter().chain(&self.cwd).cloned().collect();
        let map = |m: &serde_yaml_ng::Mapping| -> Vec<String> {
            m.iter().flat_map(|(k, v)| [scalar(k), scalar(v)]).flatten().collect()
        };
        match &self.action {
            Action::Composer(a) | Action::Php(a) => out.extend(a.iter().cloned()),
            Action::Run(c) => out.push(c.clone()),
            Action::Download(d) => out.extend([Some(d.url.clone()), d.to.clone(), d.strip.clone()].into_iter().flatten()),
            Action::Write(f) | Action::Append(f) => out.extend([f.path.clone(), f.content.clone()]),
            Action::Copy(c) => out.extend([c.from.clone(), c.to.clone()]),
            Action::Env(e) => {
                out.push(e.file.clone());
                out.extend(map(&e.values));
            }
            Action::Replace(r) => {
                out.push(r.file.clone());
                out.extend(map(&r.values));
            }
            Action::CreateDatabase(_) => {}
            Action::GitInit(g) => out.extend(g.commit.clone()),
        }
        out
    }
}

/// A YAML scalar as text (numbers and booleans included), for map keys and values.
pub fn scalar(v: &serde_yaml_ng::Value) -> Option<String> {
    match v {
        serde_yaml_ng::Value::String(s) => Some(s.clone()),
        serde_yaml_ng::Value::Number(n) => Some(n.to_string()),
        serde_yaml_ng::Value::Bool(b) => Some(b.to_string()),
        serde_yaml_ng::Value::Null => Some(String::new()),
        _ => None,
    }
}

/// Placeholder names every template can use (besides `vars.<id>` and `random:<length>`).
pub const PLACEHOLDERS: &[&str] = &[
    "project",
    "domain",
    "url",
    "www",
    "dir",
    "php_version",
    "database",
    "db.engine",
    "db.host",
    "db.port",
    "db.user",
    "db.password",
    "db.server_version",
    "venv.python",
    "mvnw",
    "java_major",
    "db.scheme",
    "db.charset",
    "db.pdo",
    "mail.host",
    "mail.port",
    "node_version",
    "pm",
    "pm.run",
    "pm.args",
    "pm.exec",
    "pm.create",
    "pm.install",
    "pm.add",
];

impl Template {
    pub fn uses_php(&self) -> bool {
        self.requires.php.is_some()
    }

    pub fn uses_node(&self) -> bool {
        self.requires.node
    }

    /// Catches mistakes when the template loads rather than halfway through a project:
    /// the schema, ids, duplicate step ids and unknown placeholders.
    fn check(&self) -> Result<()> {
        if self.schema != SCHEMA {
            return Err(Error::msg(format!("schema {} isn't supported (this LampRabit reads schema {SCHEMA})", self.schema)));
        }
        if self.id.is_empty() || !self.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            return Err(Error::msg("id must use letters, numbers, dashes and underscores"));
        }
        if self.steps.is_empty() {
            return Err(Error::msg("has no steps"));
        }
        for c in self.commands.iter().flat_map(|c| &c.list) {
            for name in placeholders(&c.run) {
                if !PLACEHOLDERS.contains(&name.as_str()) {
                    return Err(Error::msg(format!("command \"{}\": unknown placeholder {{{{{name}}}}}", c.name)));
                }
            }
        }
        if let Some(serve) = &self.serve {
            if serve.mode == crate::config::ServeMode::Dynamic {
                // A dev server runs on a language LampRabit provides.
                let language = self.requires.node || [&self.requires.python, &self.requires.java, &self.requires.go, &self.requires.rust].iter().any(|r| r.is_some());
                if !language {
                    return Err(Error::msg("serve: dynamic needs a language: requires: node, python, java, go or rust"));
                }
                if !serve.file.trim().is_empty() && !self.requires.node {
                    return Err(Error::msg("serve: file runs with node, so it needs requires: node: true"));
                }
                if serve.command.trim().is_empty() && serve.file.trim().is_empty() {
                    return Err(Error::msg("serve: dynamic needs a command or a file"));
                }
            }
            for name in placeholders(&serve.command).into_iter().chain(placeholders(&serve.file)) {
                if !PLACEHOLDERS.contains(&name.as_str()) && !name.starts_with("vars.") {
                    return Err(Error::msg(format!("serve: unknown placeholder {{{{{name}}}}}")));
                }
            }
        }
        let known = |name: &str| PLACEHOLDERS.contains(&name) || name.strip_prefix("vars.").is_some_and(|v| self.variables.iter().any(|x| x.id == v));
        let label_ok = |s: &str| !s.is_empty() && !s.starts_with('-') && !s.ends_with('-') && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        let mut names = std::collections::BTreeSet::new();
        for p in &self.processes {
            if !label_ok(&p.name) || !names.insert(p.name.as_str()) {
                return Err(Error::msg(format!("process \"{}\": names are lowercase letters, digits and dashes, each used once", p.name)));
            }
            if p.command.trim().is_empty() {
                return Err(Error::msg(format!("process {}: command is empty", p.name)));
            }
            match p.port.as_deref() {
                None | Some("auto") => {}
                Some(n) if n.parse::<u16>().is_ok_and(|n| n >= 1024) => {}
                Some(n) => return Err(Error::msg(format!("process {}: port \"{n}\" isn't \"auto\" or a number from 1024 up", p.name))),
            }
            for text in [&p.command, &p.cwd].into_iter().chain(p.env.values()) {
                if let Some(name) = placeholders(text).into_iter().find(|n| !known(n)) {
                    return Err(Error::msg(format!("process {}: unknown placeholder {{{{{name}}}}}", p.name)));
                }
            }
            if let Some(when) = &p.when {
                parse_condition(when).map_err(|e| Error::msg(format!("process {}: {e}", p.name)))?;
            }
        }
        let mut subs = std::collections::BTreeSet::new();
        for d in &self.domains {
            if !d.name.split('.').all(label_ok) || !subs.insert(d.name.as_str()) {
                return Err(Error::msg(format!("domain \"{}\": use lowercase letters, digits and dashes (like admin), each once", d.name)));
            }
            if [d.root.is_some(), d.port.is_some(), d.process.is_some()].iter().filter(|x| **x).count() > 1 {
                return Err(Error::msg(format!("domain {}: give one of root, port or process", d.name)));
            }
            if let Some(name) = &d.process {
                if !self.processes.iter().any(|p| &p.name == name && p.port.is_some()) {
                    return Err(Error::msg(format!("domain {}: process {name} isn't in processes:, or has no port", d.name)));
                }
            }
            if let Some(when) = &d.when {
                parse_condition(when).map_err(|e| Error::msg(format!("domain {}: {e}", d.name)))?;
            }
        }
        let mut ids = std::collections::BTreeSet::new();
        for (i, step) in self.steps.iter().enumerate() {
            if !ids.insert(step.key(i)) {
                return Err(Error::msg(format!("step id \"{}\" is used twice", step.key(i))));
            }
            for tool in &step.needs {
                if !["node", "git"].contains(&tool.as_str()) {
                    return Err(Error::msg(format!("step {}: needs \"{tool}\" isn't known (use node or git)", i + 1)));
                }
            }
            for text in step.strings() {
                for name in placeholders(&text) {
                    let known = PLACEHOLDERS.contains(&name.as_str())
                        || name.strip_prefix("vars.").is_some_and(|v| self.variables.iter().any(|x| x.id == v))
                        || name.strip_prefix("random:").is_some_and(|n| n.parse::<usize>().is_ok_and(|n| (1..=256).contains(&n)));
                    if !known {
                        return Err(Error::msg(format!("step {}: unknown placeholder {{{{{name}}}}}", i + 1)));
                    }
                }
            }
            if let Some(when) = &step.when {
                parse_condition(when).map_err(|e| Error::msg(format!("step {}: {e}", i + 1)))?;
            }
        }
        for v in &self.variables {
            if matches!(v.kind, VariableKind::Choice | VariableKind::Multi) && v.options.is_empty() {
                return Err(Error::msg(format!("variable {} has no options", v.id)));
            }
            for o in &v.options {
                if let Some(when) = &o.when {
                    parse_condition(when).map_err(|e| Error::msg(format!("variable {}, option {}: {e}", v.id, o.value)))?;
                }
            }
            if let Some(when) = &v.when {
                parse_condition(when).map_err(|e| Error::msg(format!("variable {}: {e}", v.id)))?;
            }
        }
        Ok(())
    }

    /// Risky things a template does, each with why; asked about before running a template
    /// that didn't ship with LampRabit.
    pub fn risks(&self) -> Vec<Risk> {
        self.steps
            .iter()
            .enumerate()
            .filter_map(|(i, s)| {
                let reasons = step_risks(s);
                (!reasons.is_empty()).then(|| Risk { step: s.key(i), heading: s.heading(), command: s.what_runs(), reasons })
            })
            .collect()
    }
}

/// `{{ name }}` occurrences in a string.
fn placeholders(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        let Some(end) = rest[start..].find("}}") else { break };
        out.push(rest[start + 2..start + end].trim().to_string());
        rest = &rest[start + end + 2..];
    }
    out
}

/// Commands that can do damage outside the new project, or run code from the internet.
const RISKY: &[(&str, &str)] = &[
    ("rm -rf", "deletes files recursively"),
    ("rm -r", "deletes files recursively"),
    ("rm -fr", "deletes files recursively"),
    ("rmdir /s", "deletes folders recursively"),
    ("rd /s", "deletes folders recursively"),
    ("del /s", "deletes files recursively"),
    ("remove-item", "deletes files"),
    ("sudo ", "runs as administrator"),
    ("runas", "runs as administrator"),
    ("mkfs", "formats a disk"),
    ("format ", "can format a disk"),
    ("dd if=", "writes raw disk data"),
    ("| sh", "runs a script piped from another command"),
    ("|sh", "runs a script piped from another command"),
    ("| bash", "runs a script piped from another command"),
    ("|bash", "runs a script piped from another command"),
    ("iex", "runs downloaded PowerShell code"),
    ("invoke-expression", "runs downloaded PowerShell code"),
    ("curl ", "downloads from the internet"),
    ("wget ", "downloads from the internet"),
    ("invoke-webrequest", "downloads from the internet"),
    ("chmod -r 777", "makes files writable by everyone"),
    ("chown -r", "changes file owners recursively"),
    ("reg ", "edits the Windows registry"),
    ("setx ", "changes environment variables for your account"),
    ("shutdown", "shuts the computer down"),
    ("reboot", "restarts the computer"),
    ("/etc/", "touches system files"),
    ("c:\\windows", "touches system files"),
    ("%systemroot%", "touches system files"),
    ("..", "reaches outside the project folder"),
    ("~/", "touches your home folder"),
    ("$home", "touches your home folder"),
    ("%userprofile%", "touches your home folder"),
    ("eval ", "runs generated code"),
];

/// Why a step is risky; one line per reason ("rm -rf" and "rm -r" are one reason).
fn step_risks(step: &Step) -> Vec<String> {
    match &step.action {
        Action::Run(cmd) => {
            let lower = cmd.to_lowercase();
            let mut reasons: Vec<String> = Vec::new();
            for (pattern, reason) in RISKY {
                if lower.contains(pattern) && !reasons.iter().any(|r| r == reason) {
                    reasons.push(reason.to_string());
                }
            }
            reasons
        }
        Action::Download(d) => {
            let mut reasons = Vec::new();
            if !d.url.starts_with("https://") {
                reasons.push("downloads over an unencrypted connection".into());
            }
            if d.sha256.is_none() && d.sha256_url.is_none() && d.sha1_url.is_none() {
                reasons.push("downloads a file without checking its checksum".into());
            }
            reasons
        }
        _ => Vec::new(),
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Risk {
    pub step: String,
    pub heading: String,
    /// What actually runs or is downloaded, shown for approval (a step's name could say anything).
    pub command: String,
    pub reasons: Vec<String>,
}

/// A parsed `if:` condition.
#[derive(Debug, PartialEq)]
enum Condition {
    Truthy(String, bool),
    Compare(String, String, bool),
}

fn parse_condition(text: &str) -> Result<Condition> {
    let text = text.trim();
    let name = |s: &str| -> Result<String> {
        let s = s.trim().trim_start_matches("{{").trim_end_matches("}}").trim();
        if s.is_empty() || !s.chars().all(|c| c.is_ascii_alphanumeric() || "._".contains(c)) {
            return Err(Error::msg(format!("can't read the condition \"{text}\"")));
        }
        Ok(s.to_string())
    };
    for (op, equal) in [("!=", false), ("==", true)] {
        if let Some((lhs, rhs)) = text.split_once(op) {
            return Ok(Condition::Compare(name(lhs)?, rhs.trim().trim_matches(['"', '\'']).to_string(), equal));
        }
    }
    Ok(match text.strip_prefix('!') {
        Some(rest) => Condition::Truthy(name(rest)?, false),
        None => Condition::Truthy(name(text)?, true),
    })
}

/// Values for placeholders and conditions.
pub struct Values(pub BTreeMap<String, String>);

impl Values {
    pub fn get(&self, name: &str) -> String {
        self.0.get(name).cloned().unwrap_or_default()
    }

    pub fn holds(&self, condition: &str) -> Result<bool> {
        let truthy = |v: &str| !v.is_empty() && v != "false" && v != "none";
        // Variables keep the value picked in the wizard under "raw.", for conditions; the
        // plain name has what steps render (a flag, say).
        let picked = |name: &str| self.0.get(&format!("raw.{name}")).cloned().unwrap_or_else(|| self.get(name));
        Ok(match parse_condition(condition)? {
            Condition::Truthy(name, want) => truthy(&picked(&name)) == want,
            Condition::Compare(name, value, equal) => (picked(&name) == value) == equal,
        })
    }

    /// Fills in `{{ name }}` placeholders; `{{random:N}}` is a fresh secret each time.
    pub fn render(&self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(start) = rest.find("{{") {
            let Some(end) = rest[start..].find("}}") else { break };
            out.push_str(&rest[..start]);
            let name = rest[start + 2..start + end].trim();
            match name.strip_prefix("random:").and_then(|n| n.parse().ok()) {
                Some(len) => out.push_str(&crate::projects::random_string(len)),
                None => out.push_str(&self.get(name)),
            }
            rest = &rest[start + end + 2..];
        }
        out.push_str(rest);
        out
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    BuiltIn,
    User,
}

/// A template as the wizard sees it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: u32,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub duration: Option<String>,
    pub source: Source,
    /// Where a user template's file is.
    pub path: Option<PathBuf>,
    pub uses_php: bool,
    pub uses_node: bool,
    /// Lowest Python, Java, Go and Rust versions the template needs (LampRabit's own).
    pub python: Option<String>,
    pub java: Option<String>,
    pub go: Option<String>,
    pub rust: Option<String>,
    pub min_php: Option<String>,
    pub extensions: Vec<String>,
    pub database: DatabaseUse,
    pub engines: Vec<DbEngine>,
    pub variables: Vec<Variable>,
    pub optional_steps: Vec<OptionalStep>,
    /// Steps that need approval before running (user templates only).
    pub risks: Vec<Risk>,
    pub done: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OptionalStep {
    pub id: String,
    pub label: String,
    pub default: bool,
    /// The step's `if:`; the wizard hides the checkbox while it doesn't hold.
    pub when: Option<String>,
}

/// A user template that couldn't be read, with why.
#[derive(Debug, Clone, Serialize)]
pub struct Broken {
    pub file: PathBuf,
    pub error: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Catalog {
    pub templates: Vec<TemplateInfo>,
    pub broken: Vec<Broken>,
    /// The folder for your own templates.
    pub folder: PathBuf,
}

pub fn folder(layout: &Layout) -> PathBuf {
    layout.root().join("templates")
}

/// Reads and checks a template's YAML, as loading one does (for the template editor).
pub fn parse_text(text: &str) -> Result<Template> {
    parse(text)
}

fn parse(text: &str) -> Result<Template> {
    let template: Template = serde_yaml_ng::from_str(text).map_err(|e| Error::msg(e.to_string()))?;
    template.check()?;
    Ok(template)
}

/// Built-in templates, then the user's (a user template with a built-in's id replaces it).
pub fn load(layout: &Layout) -> (Vec<(Template, Source, Option<PathBuf>)>, Vec<Broken>) {
    let mut all: Vec<(Template, Source, Option<PathBuf>)> = BUILT_IN
        .iter()
        .map(|(file, text)| (parse(text).unwrap_or_else(|e| panic!("built-in template {file}: {e}")), Source::BuiltIn, None))
        .collect();
    let mut broken = Vec::new();
    let mut files: Vec<PathBuf> = std::fs::read_dir(folder(layout))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "yaml" || e == "yml"))
        .collect();
    files.sort();
    for file in files {
        match std::fs::read_to_string(&file).map_err(Error::from).and_then(|t| parse(&t)) {
            Ok(t) => {
                all.retain(|(other, _, _)| other.id != t.id);
                all.push((t, Source::User, Some(file)));
            }
            Err(e) => broken.push(Broken { file, error: e.to_string() }),
        }
    }
    (all, broken)
}

pub fn catalog(layout: &Layout) -> Catalog {
    let (all, broken) = load(layout);
    let templates = all
        .into_iter()
        .map(|(t, source, path)| TemplateInfo {
            risks: if source == Source::User { t.risks() } else { Vec::new() },
            optional_steps: t
                .steps
                .iter()
                .enumerate()
                .filter(|(_, s)| s.optional)
                .map(|(i, s)| OptionalStep {
                    id: s.key(i),
                    label: s.label.clone().unwrap_or_else(|| s.heading()),
                    default: s.default.unwrap_or(true),
                    when: s.when.clone(),
                })
                .collect(),
            id: t.id.clone(),
            name: t.name.clone(),
            description: t.description.clone(),
            version: t.version,
            icon: t.icon.clone(),
            color: t.color.clone(),
            duration: t.duration.clone(),
            source,
            path,
            uses_php: t.uses_php(),
            uses_node: t.uses_node(),
            python: t.requires.python.clone(),
            java: t.requires.java.clone(),
            go: t.requires.go.clone(),
            rust: t.requires.rust.clone(),
            min_php: t.requires.php.clone().filter(|v| v != "any"),
            extensions: t.requires.extensions.clone(),
            database: t.database,
            engines: t.engines.clone(),
            variables: t.variables.clone(),
            done: t.done.clone(),
        })
        .collect();
    Catalog { templates, broken, folder: folder(layout) }
}

/// The template with this id, and whether it shipped with LampRabit.
pub fn find(layout: &Layout, id: &str) -> Result<(Template, Source)> {
    load(layout)
        .0
        .into_iter()
        .find(|(t, _, _)| t.id == id)
        .map(|(t, source, _)| (t, source))
        .ok_or_else(|| Error::msg(format!("There's no template \"{id}\". Check the templates folder.")))
}

/// A path from a template, inside `base`: absolute paths and `..` are refused, so a
/// template can't write outside the project.
pub fn inside(base: &Path, relative: &str) -> Result<PathBuf> {
    let rel = Path::new(relative);
    if rel.is_absolute() || rel.components().any(|c| !matches!(c, std::path::Component::Normal(_) | std::path::Component::CurDir)) {
        return Err(Error::msg(format!("\"{relative}\" must be a path inside the project")));
    }
    Ok(base.join(rel))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_templates_load() {
        for (file, text) in BUILT_IN {
            if let Err(e) = parse(text) {
                panic!("{file}: {e}");
            }
        }
    }

    #[test]
    fn the_example_template_is_valid() {
        let t = parse(EXAMPLE).unwrap();
        assert_eq!(t.variables.len(), 1);
        assert!(t.risks().is_empty(), "the example shouldn't ask for approval: {:?}", t.risks());
    }

    #[test]
    fn variables_render_flags_and_lists() {
        let t = parse(r#"schema: 1
id: t
name: T
variables:
  - { id: lint, label: Lint, type: bool, on: --eslint, off: --no-eslint }
  - { id: add, label: Add, type: multi, options: [react, mdx], flag: "--add {value}" }
  - id: router
    label: Router
    type: choice
    options:
      - { value: --app, label: App Router }
      - { value: --no-app, label: Pages Router, if: vars.lint == true }
steps:
  - run: x {{vars.lint}} {{vars.add}} {{vars.router}}
"#)
        .unwrap();
        let [lint, add, router] = [&t.variables[0], &t.variables[1], &t.variables[2]];
        assert_eq!(lint.render("true"), "--eslint");
        assert_eq!(lint.render("false"), "--no-eslint");
        assert_eq!(add.render("react,mdx"), "--add react,mdx");
        assert_eq!(add.render(""), "");
        assert_eq!(router.options[1].label, "Pages Router");
        assert_eq!(router.options[0].when, None);
        let values = Values(BTreeMap::from([("vars.lint".into(), "--eslint".into()), ("raw.vars.lint".into(), "true".into())]));
        assert!(values.holds("vars.lint == true").unwrap(), "conditions see the picked value");
    }

    #[test]
    fn conditions() {
        let values = Values(BTreeMap::from([("database".into(), "shop".into()), ("vars.kit".into(), "react".into())]));
        assert!(values.holds("database").unwrap());
        assert!(!values.holds("!database").unwrap());
        assert!(values.holds("vars.kit == react").unwrap());
        assert!(values.holds("vars.kit != 'vue'").unwrap());
        assert!(!values.holds("vars.missing").unwrap());
        assert!(parse_condition("rm -rf /").is_err());
    }

    #[test]
    fn rendering_fills_placeholders_and_fresh_secrets() {
        let values = Values(BTreeMap::from([("project".into(), "shop".into())]));
        assert_eq!(values.render("www/{{ project }}/{{project}}"), "www/shop/shop");
        let a = values.render("{{random:32}}");
        assert_eq!(a.len(), 32);
        assert_ne!(a, values.render("{{random:32}}"));
    }

    #[test]
    fn unknown_placeholders_and_bad_schema_are_caught() {
        let base = "schema: 1\nid: t\nname: T\nsteps:\n  - run: echo {{nope}}\n";
        assert!(parse(base).unwrap_err().to_string().contains("unknown placeholder"));
        assert!(parse("schema: 2\nid: t\nname: T\nsteps:\n  - run: echo\n").unwrap_err().to_string().contains("schema 2"));
        assert!(parse("schema: 1\nid: t\nname: T\nsteps:\n  - teleport: x\n").is_err());
    }

    #[test]
    fn processes_and_domains_are_checked() {
        let base = "schema: 1\nid: t\nname: T\nsteps:\n  - run: echo\n";
        let ok = format!("{base}processes:\n  - name: api\n    command: npm run dev -- --port {{port}}\n    port: auto\ndomains:\n  - name: api\n    process: api\n  - name: admin\n    root: admin/public\n");
        let t = parse(&ok).unwrap();
        assert_eq!(t.domains[1].target(), crate::config::DomainTarget::Files { root: "admin/public".into() });
        for (bad, why) in [
            (format!("{base}domains:\n  - name: api\n    process: nope\n"), "unknown process"),
            (format!("{base}domains:\n  - name: x\n    root: a\n    port: 3000\n"), "two targets"),
            (format!("{base}processes:\n  - name: web\n    command: x\n    port: '80'\n"), "port below 1024"),
            (format!("{base}processes:\n  - name: Web\n    command: x\n"), "uppercase name"),
            (format!("{base}processes:\n  - name: w\n    command: \"{{{{nope}}}}\"\n"), "unknown placeholder"),
        ] {
            assert!(parse(&bad).is_err(), "{why} should be refused");
        }
    }

    #[test]
    fn risky_commands_are_flagged() {
        let t = parse("schema: 1\nid: t\nname: T\nsteps:\n  - run: npm install\n  - run: curl https://x.sh | sh\n  - download: { url: 'http://x/a.zip' }\n").unwrap();
        let risks = t.risks();
        assert!(risks.iter().all(|r| r.step != "0"), "npm install isn't risky");
        let piped = risks.iter().find(|r| r.step == "1").unwrap();
        assert_eq!(piped.command, "curl https://x.sh | sh");
        assert!(piped.reasons.iter().any(|r| r.contains("piped")));
        assert!(risks.iter().any(|r| r.step == "2" && r.reasons.iter().any(|x| x.contains("checksum"))));
        // "rm -rf" also contains "rm -r": one reason, once.
        let t = parse("schema: 1\nid: t\nname: T\nsteps:\n  - run: rm -rf build\n").unwrap();
        assert_eq!(t.risks()[0].reasons, vec!["deletes files recursively".to_string()]);
    }

    #[test]
    fn paths_stay_inside_the_project() {
        let base = Path::new("/www/shop");
        assert!(inside(base, "config/app.php").is_ok());
        assert!(inside(base, "../other/x").is_err());
        assert!(inside(base, "/etc/hosts").is_err());
    }
}

/// Written next to your templates the first time the folder is opened. It ends in .txt so
/// it doesn't show up in the wizard until renamed.
pub const EXAMPLE: &str = include_str!("../templates/example.yaml.txt");

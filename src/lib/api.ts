import { invoke } from "@tauri-apps/api/core";

export type ServiceKind = "apache" | "nginx" | "caddy" | "frankenphp" | "tomcat" | "postgres" | "pgadmin" | "mongodb" | "clickhouse" | "neo4j" | "meilisearch" | "typesense" | "rustfs" | "ollama" | "minio" | "elasticsearch" | "opensearch" | "n8n" | "rabbitmq" | "mariadb" | "mailpit" | "redis" | "memcached" | "mysql";
export type Server = "apache" | "nginx" | "caddy" | "frankenphp";
export const allServers: Server[] = ["apache", "nginx", "caddy", "frankenphp"];
export type ServiceState = "stopped" | "starting" | "running" | "stopping" | "error";
export type Runtime =
  | "apache"
  | "nginx"
  | "php"
  | "mariadb"
  | "mailpit"
  | "adminer"
  | "phpmyadmin"
  | "ngrok"
  | "cloudflared"
  | "redis"
  | "composer"
  | "mysql"
  | "phpredisadmin"
  | "memcached"
  | "caddy"
  | "java"
  | "tomcat"
  | "frankenphp"
  | "postgres"
  | "python"
  | "phppgadmin"
  | "pgadmin"
  | "mongodb"
  | "mongosh"
  | "mongotools"
  | "compass"
  | "sqlite"
  | "clickhouse"
  | "neo4j"
  | "meilisearch"
  | "typesense"
  | "rustfs"
  | "ollama"
  | "minio"
  | "elasticsearch"
  | "opensearch"
  | "n8n"
  | "rabbitmq"
  | "erlang"
  | "go"
  | "rust"
  | "maven"
  | "gradle";
// Labels come from the backend's catalog (catalog.rs), loaded once before the first render
// (loadLabels), so a runtime is named in one place. An id the catalog doesn't name shows as
// itself rather than as "undefined".
const labels: Record<string, string> = {};
const byId = new Proxy(labels, { get: (target, key) => (typeof key === "string" ? (target[key] ?? key) : undefined) });
export const runtimeLabels = byId as Record<Runtime, string>;
export const serviceLabels = byId as Record<ServiceKind, string>;

/** Fills runtimeLabels and serviceLabels from the backend; call before rendering. */
export async function loadLabels() {
  const catalog = await invoke<RuntimeCatalog>("runtime_info");
  for (const r of catalog.runtimes) labels[r.id] = r.label;
}

export interface UpdateNotice {
  runtime: Runtime;
  installed: string;
  release: Release;
}

export interface ExtInfo {
  name: string;
  kind: "bundled" | "extra";
  description: string | null;
  enabled: boolean;
  installed: boolean;
  available: string | null;
  note: string | null;
}

export type TunnelProvider = "ngrok" | "cloudflare";

export interface Tunnel {
  id: string;
  provider: TunnelProvider;
  site: string | null;
  target: string;
  url: string | null;
  status: "starting" | "online" | "error";
  error: string | null;
}

export interface Release {
  runtime: Runtime;
  version: string;
  url: string;
  sha256: string | null;
  archive: "zip" | "targz";
  label: string | null;
}

/** A Services group, from the backend; items are runtime ids plus "node" and "tunnels". */
export interface ServiceGroup {
  id: string;
  title: string;
  blurb: string;
  items: string[];
}

/** What the backend says about a runtime: its place on Services and what it needs. */
export interface RuntimeInfo {
  id: Runtime;
  label: string;
  blurb: string;
  source: string;
  /** Why it can't be installed on this OS, when it can't. */
  unavailable: string | null;
  needs: string[];
  suggests: Runtime[];
}

export interface RuntimeCatalog {
  groups: ServiceGroup[];
  runtimes: RuntimeInfo[];
}

/** How one need of a runtime version gets met before it's installed. */
export type RequirementAction =
  | { kind: "ready"; version: string }
  | { kind: "activate"; version: string }
  | { kind: "install"; release: Release }
  | { kind: "installNode"; version: string }
  | { kind: "useNode"; version: string }
  | { kind: "unavailable"; reason: string };

export interface Requirement {
  /** A runtime id, or "node". */
  runtime: string;
  label: string;
  why: string;
  /** "17 or newer", "3.10 to 3.14.x"; empty when any version works. */
  range: string;
  action: RequirementAction;
}

export interface InstalledRuntime {
  runtime: Runtime;
  active: string | null;
  versions: string[];
}

export interface InstallProgress {
  runtime: Runtime;
  version: string;
  stage: "downloading" | "verifying" | "extracting" | "setup" | "done" | "error";
  downloaded: number;
  total: number | null;
}

export interface ServiceInfo {
  kind: ServiceKind;
  label: string;
  state: ServiceState;
  pid: number | null;
  port: number;
  version: string | null;
  installed: boolean;
  startedAt: number | null;
  configPath: string;
  logPath: string;
  health: Health | null;
  problem: Problem | null;
  /** Running, but not started by LampRabit (the Ollama app): it can't be stopped here. */
  external: boolean;
}

export interface Health {
  status: "checking" | "healthy" | "degraded";
  detail: string | null;
  checkedAt: number;
}

export interface PortOwner {
  pid: number;
  name: string;
  note: string | null;
  stoppable: boolean;
}

export interface Problem {
  title: string;
  detail: string;
  fixes: string[];
  port: number | null;
  owner: PortOwner | null;
  at: number;
}

export interface PortStatus {
  key: keyof Settings["ports"];
  port: number;
  free: boolean;
  usedByService: string | null;
  owner: PortOwner | null;
}

/** Where an extra domain of a project goes: the same choices as its main domain. */
export type DomainTarget =
  | { kind: "main" }
  /** A folder in the project; "" for the one the main domain serves. */
  | { kind: "files"; root: string }
  | { kind: "port"; port: number }
  | { kind: "process"; name: string };

export interface ExtraDomain {
  host: string;
  target: DomainTarget;
}

export type RestartPolicy = "never" | "on-failure" | "always";

/** A long-running command of a project (queue worker, scheduler, Vite). */
export interface ProjectProcess {
  name: string;
  /** {port} is filled in. */
  command: string;
  /** Where it runs, relative to the project; "" for the project folder. */
  cwd: string;
  /** Where it listens, if it serves anything; also $PORT. */
  port: number | null;
  env: Record<string, string>;
  restart: RestartPolicy;
  /** Starts and stops with the project's app. */
  withApp: boolean;
  /** Starts when LampRabit starts. */
  atLaunch: boolean;
  /** A crontab line ("0 3 * * *"); "" for none. */
  schedule: string;
}

export interface Site {
  name: string;
  domain: string;
  path: string;
  /** Directory Apache serves: the project's public/ folder when it has one. */
  docroot: string;
  ssl: boolean;
  url: string;
  modified: number;
  /** Set for built-in tools rather than www/ folders. */
  tool: "adminer" | "phpmyadmin" | "phpredisadmin" | "mailpit" | null;
  proxy: string | null;
  /** How the project's process runs, once set up (or started once). */
  app: NodeApp | null;
  /** Has a package.json, so it has scripts and can run a dev server. */
  nodeProject: boolean;
  language: Language;
  /** dynamic: the domain forwards to the project's dev server; static: the web server serves the files. */
  serveMode: ServeMode;
  /** Where the domain goes right now: a port the project listens on, or null for its files. */
  route: Route | null;
  /** Every port the project's processes listen on, whoever started them. */
  ports: DetectedPort[];
  /** The port picked for the domain when there are several; null chooses automatically. */
  routePort: number | null;
  /** PHP version chosen for the site; null follows the active version. */
  php: string | null;
  server: Server;
  /** Node version from the project's .nvmrc / .node-version. */
  node: string | null;
  /** "Laravel", "WordPress", "Symfony", "PHP", "Static"…, from the project's files. */
  framework: string | null;
  /** The checked-out git branch (or short commit). */
  gitBranch: string | null;
  /** The database the project's config names. */
  database: string | null;
  /** What the project's version file asks for (Python, Java, Go, Rust). */
  versionRequest: { version: string; file: string } | null;
  /** The installed version it runs on: the request's match, else the active version. */
  languageVersion: string | null;
}

export interface NodeStatus {
  manager: "nvm-windows" | "nvm" | null;
  managerVersion: string | null;
  current: string | null;
  installed: string[];
  needsElevation: boolean;
}

export interface NodeRelease {
  version: string;
  major: number;
  lts: string | null;
}

export interface NewProject {
  name: string;
  /** A template id from listTemplates. */
  template: string;
  php: string | null;
  server: Server | null;
  ssl: boolean;
  database: string | null;
  /** The server the database goes on; the default database when null. */
  engine: DbEngine | null;
  /** Values for the template's variables, by id. */
  variables: Record<string, string>;
  /** Optional steps by id: run or not. */
  optional: Record<string, boolean>;
  /** The user accepted the risky steps of a template that didn't ship with LampRabit. */
  approveRisky: boolean;
  /** Node templates. */
  node: string | null;
  packageManager: PackageManager | null;
}

export interface TemplateVariable {
  id: string;
  label: string;
  help: string | null;
  type: "text" | "choice" | "bool" | "multi";
  /** Options shown only while their `when` holds. */
  options: { value: string; label: string; when: string | null }[];
  /** multi: joins the picked values (the wizard stores them comma-separated). */
  separator: string | null;
  default: string | null;
  /** Shown only while this condition holds (same syntax as a step's if:). */
  when: string | null;
  required: boolean;
}

/** Whether a folder can hold the LampRabit folder (Settings → Move). */
export interface MoveCheck {
  problems: string[];
  size: number;
  free: number | null;
  sameDrive: boolean;
  fileSystem: string | null;
}

/** A program that keeps the LampRabit folder busy. */
export interface Holder {
  pid: number;
  name: string;
  path: string;
}

export type DbEngine = "mariadb" | "mysql" | "postgres" | "sqlite" | "mongodb";

/** A database dump in <root>/backups/<engine>/. */
export interface Backup {
  engine: DbEngine;
  database: string;
  path: string;
  size: number;
  /** Unix time in milliseconds. */
  created: number;
}

export interface ProjectTemplate {
  id: string;
  name: string;
  description: string;
  version: number;
  icon: string | null;
  color: string | null;
  duration: string | null;
  source: "builtin" | "user";
  path: string | null;
  usesPhp: boolean;
  usesNode: boolean;
  /** Lowest Python / Java version, for templates that run on LampRabit's own. */
  python: string | null;
  java: string | null;
  go: string | null;
  rust: string | null;
  minPhp: string | null;
  extensions: string[];
  database: "none" | "optional" | "required";
  /** Database servers the template can set a project up for. */
  engines: DbEngine[];
  variables: TemplateVariable[];
  optionalSteps: { id: string; label: string; default: boolean; when: string | null }[];
  /** Steps needing approval (templates you added yourself). */
  risks: { step: string; heading: string; command: string; reasons: string[] }[];
  done: string | null;
}

export interface TemplateCatalog {
  templates: ProjectTemplate[];
  broken: { file: string; error: string }[];
  folder: string;
}

export interface ProjectProgress {
  name: string;
  step: string | null;
  line: string | null;
}

export interface PoolStatus {
  version: string;
  port: number;
  workers: number;
  alive: number;
}

export interface SitesInfo {
  sites: Site[];
  tools: Site[];
  tld: string;
  needsApply: boolean;
  hostsOk: boolean;
  manageHosts: boolean;
  /** null while no site uses HTTPS. */
  caTrusted: boolean | null;
  caPath: string;
  /** Folders in www that can't be served, and why. */
  unserved: { name: string; reason: string }[];
  /** Tool domains (adminer.test…) that a folder of the same name has taken. */
  shadowedTools: string[];
}

export interface LogHistory {
  lines: RawLogLine[];
  cursor: number;
}

export interface StatusEvent {
  kind: ServiceKind;
  state: ServiceState;
  message: string | null;
}

export type LogService = "apache" | "nginx" | "caddy" | "frankenphp" | "tomcat" | "postgres" | "pgadmin" | "mongodb" | "clickhouse" | "neo4j" | "meilisearch" | "typesense" | "rustfs" | "ollama" | "minio" | "elasticsearch" | "opensearch" | "n8n" | "rabbitmq" | "php" | "mariadb" | "mysql" | "mailpit" | "redis" | "memcached" | "system" | "project" | "dumps";

/** A raw log line from a service's output or one of its log files. */
export interface RawLogLine {
  id: number;
  service: LogService;
  /** "command" is what LampRabit ran for a project, and how it ended. */
  stream: LogStream;
  line: string;
  /** For service "project": which project. */
  project?: string;
}

export type LogStream = "output" | "error" | "access" | "command";

/** What a project is written in; decides its runtime and how it's served. */
export type Language = "php" | "node" | "java" | "python" | "go" | "rust" | "static";
export type ServeMode = "dynamic" | "static";
export interface Route {
  port: number;
  /** 127.0.0.1, or [::1] for servers listening on IPv6 only. */
  host: string;
}

export interface DetectedPort extends Route {
  pid: number;
  /** node, php… */
  process: string;
  command: string;
}

export type AppMode = "command" | "file";
export type PackageManager = "npm" | "pnpm" | "yarn" | "bun";

/** How a Node project runs (Sites → Node app). */
export interface NodeApp {
  mode: AppMode;
  /** Command mode; `{port}` is filled in when it starts. */
  command: string;
  file: string;
  watch: boolean;
  port: number;
  node: string | null;
  packageManager: PackageManager | null;
  /** https://<project>.test forwards to the app. */
  serve: boolean;
}

export interface AppStatus {
  name: string;
  state: "stopped" | "starting" | "running" | "crashed";
  pid: number | null;
  port: number | null;
  since: number | null;
  node: string | null;
  exitCode: number | null;
  /** A command running for the project. */
  task: string | null;
}

export interface AppInfo {
  config: NodeApp;
  configured: boolean;
  scripts: Record<string, string>;
  detectedPackageManager: PackageManager;
  nodeVersions: string[];
  projectNode: string | null;
  framework: string | null;
  status: AppStatus;
}

/** A command run for a project, from its run history. */
export interface CommandRun {
  label: string;
  command: string;
  started: number;
  durationMs: number;
  /** null when it was stopped. */
  exitCode: number | null;
}

/** A project's process (or dev server) that crashed: the "process-crashed" event. */
export interface Crash {
  name: string;
  exitCode: number | null;
  lines: string[];
  restarting: boolean;
  gaveUp: boolean;
}

export interface AppOutput {
  name: string;
  line: string;
  stream: LogStream;
}

export interface ProjectCommand {
  group: string;
  name: string;
  command: string;
  description: string | null;
  origin: "yours" | "template" | "discovered";
}

export interface ProjectCommands {
  commands: ProjectCommand[];
  problems: string[];
  sources: string[];
}

/** What the Overview shows about a project beyond the site list (project_facts). */
/** Whether a project's database can be used; from databaseStatus. */
export interface DbStatus {
  engine: "mariadb" | "mysql" | "postgres" | "sqlite";
  label: string;
  database: string;
  installed: boolean;
  running: boolean;
  /** null when it can't be told (PostgreSQL while stopped). */
  exists: boolean | null;
}

export interface ProjectFacts {
  package: string | null;
  description: string | null;
  version: string | null;
  /** Installed framework version, or the declared range when not installed. */
  frameworkVersion: string | null;
  phpRequirement: string | null;
  nodeRequirement: string | null;
  /** outdated: installed, but the lock file changed since (a pull, a branch switch). */
  dependencies: { manager: string; count: number; devCount: number; installed: boolean; outdated: boolean }[];
  envFile: string | null;
  /** [key, value] pairs; secrets are never included. */
  env: [string, string][];
  git: { remote: string | null; commit: [string, string, number] | null; changes: number | null } | null;
  /** Name of the template it was created from. */
  template: string | null;
}

export interface UserCommand {
  name: string;
  run: string;
}

export interface Settings {
  rootDir: string;
  tld: string;
  ports: {
    http: number;
    https: number;
    /** MariaDB's port (the name predates MySQL support). */
    mysql: number;
    /** Oracle MySQL, next to MariaDB. */
    mysqlServer: number;
    phpFpm: number;
    /** Each web server's own ports, used while another server is the primary one. */
    nginxHttp: number;
    nginxHttps: number;
    apacheHttp: number;
    apacheHttps: number;
    caddyHttp: number;
    caddyHttps: number;
    /** Caddy's admin API (reload, stop). */
    caddyAdmin: number;
    frankenphpHttp: number;
    frankenphpHttps: number;
    frankenphpAdmin: number;
    /** Tomcat's HTTP connector, behind the web servers. */
    tomcatHttp: number;
    tomcatShutdown: number;
    postgres: number;
    pgadmin: number;
    mongodb: number;
    clickhouseHttp: number;
    clickhouseTcp: number;
    neo4jHttp: number;
    neo4jBolt: number;
    meilisearch: number;
    typesense: number;
    rustfs: number;
    rustfsConsole: number;
    ollama: number;
    minio: number;
    minioConsole: number;
    elasticsearch: number;
    elasticsearchTransport: number;
    opensearch: number;
    opensearchTransport: number;
    n8n: number;
    rabbitmq: number;
    rabbitmqManagement: number;
    mailSmtp: number;
    mailUi: number;
    redis: number;
    memcached: number;
    /** Where PHP's dump() sends dumps for Logs. */
    dumps: number;
  };
  versions: Partial<Record<Runtime, string | null>>;
  /** The web servers that run; the first is the primary one (main ports, localhost, tools). */
  webServers: Server[];
  phpWorkers: number;
  httpsByDefault: boolean;
  manageHosts: boolean;
  routeMail: boolean;
  checkUpdates: boolean;
  closeToTray: boolean;
  addToPath: boolean;
  startServicesOnLaunch: boolean;
  editor: string;
  /** `worker`: FrankenPHP worker mode (the app stays in memory between requests). */
  sites: Record<string, { ssl: boolean | null; php: string | null; server: Server | null; worker?: boolean; main?: DomainTarget | null; domains?: ExtraDomain[]; processes?: ProjectProcess[]; phpIni?: Record<string, string>; xdebug?: boolean }>;
  profiles: StackProfile[];
  /** What `mysql`, new projects and the database tools use first. */
  defaultDatabase: Database;
  /** Object storage folders by service id (RustFS, MinIO); data/<id> when absent. */
  storage: Record<string, string>;
  /** Services (by id) started again when they crash, a few times in a row at most. */
  autoRestart: string[];
  /** dump() and dd() go to Logs instead of the page. */
  catchDumps: boolean;
  /** Log files over logMaxMb are rotated, logKeep old copies kept, the folder under logTotalMb. */
  logMaxMb: number;
  logKeep: number;
  logTotalMb: number;
}

/** One node of a caught dump's value, as dumps.rs renders VarDumper's Data. */
export interface DumpNode {
  kind: "string" | "int" | "float" | "bool" | "null" | "array" | "object" | "resource" | "enum" | "more";
  class?: string;
  value?: string;
  count?: number;
  cut?: number;
  children?: { key: string; node: DumpNode }[];
}

/** A dump() caught from a project. */
export interface Dump {
  id: number;
  time: string;
  project: string | null;
  file: string | null;
  line: number | null;
  /** "GET /users", or the console command. */
  origin: string | null;
  tree: DumpNode;
}

export type Database = "mariadb" | "mysql";

/** A named set of services started and stopped together. */
/** A project's lamprabit.toml: its setup, to commit and share. Subdomains go by their first label. */
export interface ProjectFile {
  php?: string;
  server?: Server;
  ssl?: boolean;
  phpIni?: Record<string, string>;
  main?: DomainTarget;
  domains?: { name: string; target: DomainTarget }[];
  processes?: ProjectProcess[];
}

export interface StackProfile {
  /** Project processes it starts too, as "project/process". */
  processes?: string[];
  name: string;
  services: ServiceKind[];
}

/** CPU (like `top`: 100 is one full core) and memory (bytes) of one service, or "php". */
export interface Usage {
  id: ServiceKind | "php";
  cpu: number;
  memory: number;
  processes: number;
}

/** Removing LampRabit: folders deleted and kept (Settings → Troubleshooting). */
export interface UninstallPlan {
  remove: string[];
  keep: string[];
}

export interface UninstallReport {
  kept: string[];
  /** What couldn't be undone, each saying what to do. */
  problems: string[];
}

export interface EnvPaths {
  root: string;
  www: string;
  logs: string;
  settingsFile: string;
  os: "windows" | "macos" | "linux";
}

export const api = {
  getSettings: () => invoke<Settings>("get_settings"),
  takeStartupWarnings: () => invoke<string[]>("take_startup_warnings"),
  saveSettings: (settings: Settings) => invoke<Settings>("save_settings", { settings }),
  envPaths: () => invoke<EnvPaths>("env_paths"),
  listServices: () => invoke<ServiceInfo[]>("list_services"),
  startService: (kind: ServiceKind) => invoke<void>("start_service", { kind }),
  stopService: (kind: ServiceKind) => invoke<void>("stop_service", { kind }),
  restartService: (kind: ServiceKind) => invoke<void>("restart_service", { kind }),
  listSites: () => invoke<SitesInfo>("list_sites"),
  applySites: () => invoke<void>("apply_sites"),
  setSiteSsl: (name: string, ssl: boolean) => invoke<void>("set_site_ssl", { name, ssl }),
  setSitePhp: (name: string, version: string | null) => invoke<void>("set_site_php", { name, version }),
  setLanguageVersion: (name: string, version: string | null) => invoke<void>("set_language_version", { name, version }),
  setSiteServer: (name: string, server: Server | null) => invoke<void>("set_site_server", { name, server }),
  setSiteWorker: (name: string, enabled: boolean) => invoke<void>("set_site_worker", { name, enabled }),
  restartFrankenphpWorkers: () => invoke<void>("restart_frankenphp_workers"),
  phpStatus: () => invoke<PoolStatus[]>("php_status"),
  createProject: (request: NewProject) => invoke<{ url: string; note: string | null }>("create_project", { request }),
  listTemplates: () => invoke<TemplateCatalog>("list_templates"),
  appInfo: (name: string) => invoke<AppInfo>("app_info", { name }),
  appSave: (name: string, config: NodeApp) => invoke<void>("app_save", { name, config }),
  appStart: (name: string) => invoke<AppStatus>("app_start", { name }),
  appStop: (name: string) => invoke<void>("app_stop", { name }),
  appRestart: (name: string) => invoke<AppStatus>("app_restart", { name }),
  appInstall: (name: string) => invoke<void>("app_install", { name }),
  appStatuses: () => invoke<AppStatus[]>("app_statuses"),
  appLogs: (name: string) => invoke<string[]>("app_logs", { name }),
  appFreePort: () => invoke<number>("app_free_port"),
  setSiteDomains: (name: string, domains: ExtraDomain[]) => invoke<void>("set_site_domains", { name, domains }),
  /** Where the main domain goes; null follows what's running. */
  setSiteMain: (name: string, target: DomainTarget | null) => invoke<void>("set_site_main", { name, target }),
  /** The project's folders ("public", "admin/public"), for folder pickers. */
  projectFolders: (name: string) => invoke<string[]>("project_folders", { name }),
  setSiteProcesses: (name: string, processes: ProjectProcess[]) => invoke<void>("set_site_processes", { name, processes }),
  /** Statuses and output use the key "project/process" (`processKey`). */
  processStart: (name: string, process: string) => invoke<AppStatus>("process_start", { name, process }),
  processStop: (name: string, process: string) => invoke<void>("process_stop", { name, process }),
  processRestart: (name: string, process: string) => invoke<AppStatus>("process_restart", { name, process }),
  processesAll: (name: string, start: boolean) => invoke<void>("processes_all", { name, start }),
  projectCommands: (name: string) => invoke<ProjectCommands>("project_commands", { name }),
  projectFacts: (name: string) => invoke<ProjectFacts>("project_facts", { name }),
  databaseStatus: (name: string) => invoke<DbStatus | null>("database_status", { name }),
  createProjectDatabase: (name: string) => invoke<void>("create_project_database", { name }),
  setProjectEnv: (name: string, key: string, value: string) => invoke<void>("set_project_env", { name, key, value }),
  portsListening: (ports: number[]) => invoke<number[]>("ports_listening", { ports }),
  setSitePhpIni: (name: string, values: Record<string, string>, xdebug: boolean) => invoke<void>("set_site_php_ini", { name, values, xdebug }),
  renameProject: (name: string, newName: string) => invoke<void>("rename_project", { name, newName }),
  deleteProject: (name: string, dropDatabase: boolean) => invoke<void>("delete_project", { name, dropDatabase }),
  setSiteRoute: (name: string, port: number | null) => invoke<void>("set_site_route", { name, port }),
  runProjectCommand: (name: string, label: string, command: string) => invoke<void>("run_project_command", { name, label, command }),
  stopProjectCommand: (name: string) => invoke<void>("stop_project_command", { name }),
  commandHistory: (name: string) => invoke<CommandRun[]>("command_history", { name }),
  /** Unix ms of each scheduled process's next run, by process name. */
  processNextRuns: (name: string) => invoke<Record<string, number>>("process_next_runs", { name }),
  saveProjectCommands: (name: string, commands: UserCommand[]) => invoke<void>("save_project_commands", { name, commands }),
  openTemplatesFolder: () => invoke<void>("open_templates_folder"),
  /** A template that recreates the project, as YAML. */
  templateFromProject: (name: string) => invoke<string>("template_from_project", { name }),
  /** Checks and saves a template; returns its id. */
  saveTemplate: (yaml: string, replace: boolean) => invoke<string>("save_template", { yaml, replace }),
  tunnelStart: (provider: TunnelProvider, site: string | null, port: number | null) =>
    invoke<Tunnel>("tunnel_start", { provider, site, port }),
  tunnelStop: (id: string) => invoke<void>("tunnel_stop", { id }),
  tunnelList: () => invoke<Tunnel[]>("tunnel_list"),
  ngrokTokenSaved: () => invoke<boolean>("ngrok_token_saved"),
  nodeStatus: () => invoke<NodeStatus>("node_status"),
  nodeAvailable: () => invoke<NodeRelease[]>("node_available"),
  nodeInstall: (version: string) => invoke<void>("node_install", { version }),
  nodeUse: (version: string) => invoke<void>("node_use", { version }),
  nodeUninstall: (version: string) => invoke<void>("node_uninstall", { version }),
  nvmInstall: () => invoke<void>("nvm_install"),
  runtimeUpdates: () => invoke<UpdateNotice[]>("runtime_updates"),
  phpExtensions: (version: string) => invoke<ExtInfo[]>("php_extensions", { version }),
  setPhpExtension: (version: string, name: string, enabled: boolean) =>
    invoke<void>("set_php_extension", { version, name, enabled }),
  installPhpExtension: (version: string, name: string) => invoke<void>("install_php_extension", { version, name }),
  openTerminal: (dir: string | null) => invoke<void>("open_terminal", { dir }),
  openInEditor: (dir: string) => invoke<void>("open_in_editor", { dir }),
  vcRuntimeStatus: () => invoke<{ applicable: boolean; installed: boolean; version: string | null }>("vc_runtime_status"),
  vcRuntimeInstall: () => invoke<void>("vc_runtime_install"),
  setNgrokToken: (token: string | null) => invoke<void>("set_ngrok_token", { token }),
  /** Resolves to a note when browsers couldn't be covered (Linux without certutil). */
  trustCa: () => invoke<string | null>("trust_ca"),
  listDatabases: (engine: DbEngine) => invoke<string[]>("list_databases", { engine }),
  listBackups: () => invoke<Backup[]>("list_backups"),
  backupDatabase: (engine: DbEngine, database: string) => invoke<Backup>("backup_database", { engine, database }),
  restoreBackup: (engine: DbEngine, path: string, database: string) => invoke<void>("restore_backup", { engine, path, database }),
  deleteBackup: (path: string) => invoke<void>("delete_backup", { path }),
  openCompass: () => invoke<void>("open_compass"),
  ollamaElsewhere: () => invoke<string | null>("ollama_elsewhere"),
  /** True on the first run, until the LampRabit folder's place is confirmed. */
  firstRun: () => invoke<boolean>("first_run"),
  finishFirstRun: () => invoke<void>("finish_first_run"),
  checkMoveTarget: (target: string) => invoke<MoveCheck>("check_move_target", { target }),
  environmentInUse: () => invoke<Holder[]>("environment_in_use"),
  stopEnvironmentHolder: (pid: number) => invoke<void>("stop_environment_holder", { pid }),
  moveEnvironment: (target: string) => invoke<Settings>("move_environment", { target }),
  openDbClient: (database?: Database) => invoke<void>("open_db_client", { database: database ?? null }),
  startAll: () => invoke<string[]>("start_all"),
  /** Stops `stop`, then starts `start`; resolves to one message per failed start. */
  applyServices: (start: ServiceKind[], stop: ServiceKind[]) => invoke<string[]>("apply_services", { start, stop }),
  serviceUsage: () => invoke<Usage[]>("service_usage"),
  stopAll: () => invoke<void>("stop_all"),
  uninstallPlan: () => invoke<UninstallPlan>("uninstall_plan"),
  uninstall: () => invoke<UninstallReport>("uninstall"),
  checkPort: (port: number) => invoke<boolean>("check_port", { port }),
  portStatus: () => invoke<PortStatus[]>("port_status"),
  stopProcess: (pid: number) => invoke<void>("stop_process", { pid }),
  clearProblem: (kind: ServiceKind) => invoke<void>("clear_problem", { kind }),
  diagnostics: (redact: boolean) => invoke<string>("diagnostics", { redact }),
  saveText: (path: string, contents: string) => invoke<void>("save_text", { path, contents }),
  logHistory: () => invoke<LogHistory>("log_history"),
  /** One source ("apache", "dumps", "project:shop"), or every log with none. */
  clearLogs: (source?: string) => invoke<void>("clear_logs", { source: source ?? null }),
  listDumps: () => invoke<Dump[]>("list_dumps"),
  setCatchDumps: (enabled: boolean) => invoke<void>("set_catch_dumps", { enabled }),
  runtimeInfo: () => invoke<RuntimeCatalog>("runtime_info"),
  /** What `version` (default: the active one, else the newest download) needs. */
  projectFile: (name: string) => invoke<{ file: ProjectFile; matches: boolean } | null>("project_file", { name }),
  saveProjectFile: (name: string) => invoke<void>("save_project_file", { name }),
  applyProcesses: (start: string[], stop: string[]) => invoke<string[]>("apply_processes", { start, stop }),
  runtimeRequirements: (runtime: Runtime, version?: string) => invoke<Requirement[]>("runtime_requirements", { runtime, version: version ?? null }),
  runtimeCatalog: (runtime: Runtime, fresh = false) => invoke<Release[]>("runtime_catalog", { runtime, fresh }),
  installedRuntimes: () => invoke<InstalledRuntime[]>("installed_runtimes"),
  installRuntime: (release: Release) => invoke<void>("install_runtime", { release }),
  removeRuntime: (runtime: Runtime, version: string) => invoke<void>("remove_runtime", { runtime, version }),
  setActiveVersion: (runtime: Runtime, version: string) =>
    invoke<void>("set_active_version", { runtime, version }),
};

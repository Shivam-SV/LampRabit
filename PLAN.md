# LampRabit — implementation plan

A Laragon/XAMPP-style local dev environment built with Tauri v2 (Rust) and React.

## Decisions

| Topic | Decision |
|---|---|
| Platforms | Windows, macOS and Linux from day one; OS-specific code is isolated behind `cfg` |
| Binaries | Downloaded on demand from a per-OS version manifest, sha256-verified |
| MVP stack | Apache + PHP + MariaDB (MySQL added in phase 9) |
| PHP integration | Windows: thread-safe PHP loaded into Apache as `mod_php`. macOS/Linux: `php-fpm` + `mod_proxy_fcgi` |
| Binary sources | Windows: Apache Lounge, nginx.org, windows.php.net, mariadb.org, MySQL CDN. Linux: LampRabit's own CI builds of Apache, Nginx and PHP (`build/linux/`), official MariaDB and MySQL tarballs. macOS: still to do |
| Frontend | React 19, TypeScript, Vite, Tailwind v4, TanStack Query, Zustand |

## Environment layout (default `C:\lampRabit` / `~/lampRabit`)

```
bin/<runtime>/<version>/   etc/<runtime>/   www/   data/mariadb/   logs/   tmp/   ssl/
```

## Phases

- [x] **0. Scaffold**: Tauri v2 + React + TS + Tailwind
- [x] **1. Service supervisor**: start/stop/status, port-conflict check, log streaming, cleanup on exit
- [x] **2. Runtime manager**: manifest, download with progress, verify, extract, switch active version
- [x] **3. Config templating**: httpd.conf / php.ini / my.ini / php-fpm.conf from templates; MariaDB data-dir init
- [x] **4. Sites**: `www/foo` → `foo.test` (+ sub-domains, `public/` detection), hosts file kept in sync when Apache starts (one elevation prompt, managed block only), local CA + per-site HTTPS with a global default, System log stream persisted to `logs/lampRabit.log`, log history read from disk with clear, OS-specific settings hidden elsewhere
- [x] **5. Developer utilities**: Adminer / phpMyAdmin at `adminer.test`; Mailpit as a managed service (SMTP 1025, UI 8025) with PHP `mail()` routed to it (Windows: `SMTP`/`smtp_port`; macOS/Linux: `sendmail_path = mailpit sendmail`) and a panel explaining SMTP vs `mail()` settings for frameworks
- [x] **6. Feature set** (see below)
- [x] **7. Comfort and PHP tooling** (see below)
- [x] **8. Release**: installers (NSIS/MSI, dmg, AppImage/deb), signed auto-updates, CI on every push, tag-triggered draft releases (see RELEASING.md)
- [x] **Polish**: animated logo (ears, eyes, nose every few minutes), page/dialog/list motion, Motion setting (System / Full / Reduced / Off)
- [x] **9. Linux parity and developer dashboard** (see below)
- [ ] **10. macOS parity, stability on every OS, safe uninstall** (macOS part in the backlog, the user's choice 2026-11-05)
- [x] **11. Service foundation and web servers: Caddy, FrankenPHP, Tomcat (with Java)** (2026-11-05: runtime needs and platform gaps from `catalog.rs`; the frontend's runtime and service labels load from it)
- [x] **12. Projects: several domains, processes, Node.js serving** (2026-11-05: stack profiles list project processes; `lamprabit.toml` saved and applied from a project's Settings)
- [x] **13. Logs redesign and dump catcher** (2026-11-05: dump catcher, source sidebar, virtualised list, line details, per-source clear, rotation)
- [ ] **14. Movable tab bar**
- [ ] **15. More databases: PostgreSQL, MongoDB, SQLite, ClickHouse, Neo4j, backups**
- [x] **16. More services: Meilisearch, Typesense, RustFS, MinIO, OpenSearch/Elasticsearch, RabbitMQ, Ollama, n8n**
- [x] **17. Templates and more frameworks** (2026-11-05: `processes:` and `domains:` in templates, a template editor, Save as a template)
- [ ] **18. Onboarding**
- [ ] **19. Project doctor**
- [ ] **20. Headless core and the `lamprabit` CLI**
- [ ] **21. MCP server**
- [ ] **22. Git**
- [ ] **23. Dependency manager** (part of 25)
- [ ] **24. Languages: Python, Go, Rust, Java**
- [ ] **25. Package managers: npm, pnpm, Yarn, Bun, Deno, pip, conda, uv, Maven, Gradle, Cargo**
- [x] **26. Movable environment folder** (2026-11-05: the first run asks where the folder goes)
- [ ] **Later**: Git workflow builder, deployment checklists

**Order (decided 2026-11-01):** services before everything else: 11 → 15 → 16 → 24 → 25 → 22, then the rest. The numbers stay so older references keep working.

## Product evolution (decided 2026-10-29)

Phases 10 and up come from an exported product spec (`Preject-features-and-plan.md`), checked against the code. About a third of it already existed: the supervisor, Apache/Nginx, PHP per project, extensions, MariaDB/MySQL, Redis, profiles and the dashboard. The decisions below are the maintainer's answers to the points where the spec was unclear or contradicted earlier decisions.

| Topic | Decision |
|---|---|
| Order | Stable on all three OSes first (macOS builds), then all services (web servers, databases, extra services), then project tooling; CLI/MCP and Git/dependency panels come last |
| FrankenPHP and Caddy | Brought back (reverses 7.3): both become web servers next to Apache and Nginx |
| In-app database manager | Not now. Adminer, phpMyAdmin and phpRedisAdmin stay the database UIs; Redis stays on phpRedisAdmin |
| "DD and dump catcher" | PHP `dump()` / `dd()` output is caught and shown in the Logs tab, which is redesigned with better filters and options |
| Uninstall | Keeps `www/` (projects) and `data/` (databases) and removes everything else LampRabit created |
| Docking | No dockable panels; only the tab bar can be moved to the bottom, top, left or right |
| CLI | Works whether or not the app is open. The CLI and the window show exactly the same state, because both talk to one backend process |
| `lamprabit.toml` in projects | Only if a feature needs it (phase 12 decides) |
| Reference | FlyEnv (github.com/xpf0000/FlyEnv) covers many of the same services; check how it gets binaries and configures each service before designing one here. Check its licence before copying any code or templates |

## Phase 10: macOS parity, stability on every OS, safe uninstall

macOS was in preview because only Redis, Mailpit, ngrok, cloudflared and Composer had macOS downloads; Apache, Nginx, PHP, MariaDB and MySQL had none.

### 10.1 macOS runtimes, Redis and Memcached (scripts done 2026-10-29, first CI run pending)
- Compared with FlyEnv and EnvKit first. FlyEnv uses the same Windows sources as LampRabit, but on Linux/macOS it takes static-php-cli's prebuilt PHP (months behind), a stale third-party Nginx, and Homebrew/system packages for Apache, Redis and Memcached; EnvKit is closed source and bundles everything (Windows and Apple Silicon only). Decision: keep building our own, extended to macOS.
- ✅ `binaries.yml` builds Apache, Nginx, PHP, Redis and Memcached for Linux and macOS, x86_64 and aarch64 each (macOS: `@rpath` for APR, ad hoc re-signing, no file AIO), and repackages Memcached for Windows. The app reads Apache/Nginx/PHP from the manifest on macOS too.
- ✅ Real Redis on Windows (community redis-windows build) instead of Garnet + .NET; `redis-cli` comes with it; Garnet installs from 0.1 are removed on start. Verified locally: Windows path with spaces for `--dir`, `INFO keyspace`, saving.
- ✅ Memcached as a downloadable runtime everywhere (was: system install on Linux/macOS, nothing on Windows). Verified locally: Windows build (set/get/version) and the Linux build (links only glibc).
- ✅ PHP `memcached` + `igbinary`: compiled into the Linux/macOS builds (the build fails if missing), and the `memcached` PECL download in the Windows extension manager.
- ⏳ Push and watch the first CI run; test the macOS builds on a Mac (Apache, Nginx, PHP-FPM, Redis, Memcached), then mark macOS out of preview for those.
- ⏳ MariaDB ships no macOS tarballs: build it in CI too (heavy, cache aggressively), or use a detected Homebrew install. Spike both. MySQL publishes macOS tarballs (add to `mysql_releases`).
### 10.2 macOS platform pieces
- Hosts file and CA trust (`security add-trusted-cert`, an admin prompt), ports below 1024 (macOS allows them unprivileged since Mojave, but verify), terminal launcher (Terminal.app / iTerm), shims on PATH (`~/.zprofile`), process groups, `reap_orphans`.
- Gatekeeper: downloaded runtimes carry `com.apple.quarantine`; remove it after the checksum check.
- A macOS CI job that installs and starts each runtime (smoke test), like the existing checks.
### 10.3 Stability on all OSes
- ✅ A cross-platform smoke test (2026-11-04): `lamprabit --smoke-test <folder> <runtime>…` (smoke.rs) installs each runtime with the versions its needs call for into a throwaway folder (own settings, every port +20000), then start → healthy → second start ignored → stop → port free for every service, and one killed service has to come back through "Restart after a crash" and stay stopped after Stop. `.github/workflows/smoke.yml` runs it on Windows, Linux (Xvfb) and macOS for pull requests that touch the backend, weekly with the heavy services, and by hand; platform gaps are reported as skipped with their reason.
- A compatibility matrix (runtime × OS × arch, with the reason when one isn't available) in the docs, generated from the catalogs where possible.
### 10.4 Uninstall ✅ (2026-10-30)
- `uninstall.rs` removes the folders LampRabit creates under the root (`bin`, `etc`, `logs`, `tmp`, `ssl`, `shims`, nothing else), the root itself only if empty, the settings and (Windows uninstaller) WebView data, the hosts block, the trusted CA (system store and, on Linux, the browsers' NSS databases), the PATH entry, the ngrok token and the Linux sysctl file. Keeps `www/` and `data/`.
- Windows: NSIS `NSIS_HOOK_PREUNINSTALL` (src-tauri/windows/hooks.nsh) asks to close the app and runs `LampRabit.exe --uninstall-cleanup`; skipped for updates (`/UPDATE`). The uninstaller itself removes the autostart entry.
- Every OS: Settings → Troubleshooting → "Remove LampRabit from this computer" stops services cleanly, shows what is deleted and kept, reports what couldn't be undone, then quits. Linux does the system files in one pkexec prompt.
- ✅ MSI (2026-10-31): `windows/cleanup.wxs` runs `--uninstall-cleanup` as a deferred, impersonated custom action before RemoveFiles, on a real uninstall only (not for upgrades); linked through `bundle.windows.wix.componentRefs`.
- The .deb's removal can't reach per-user files, so Linux users use the Settings action.

## Sources for phases 11, 15, 16, 24, 25 (checked 2026-11-01)

FlyEnv (BSD-3) supports nearly all of these, but it downloads from its own mirror and uses Homebrew/MacPorts on macOS. That conflicts with 9.2 (official sources, pinned checksums), so FlyEnv is a reference for configs and start commands only.

| Runtime | Source | Checksum | Gaps |
|---|---|---|---|
| Caddy | GitHub caddyserver/caddy | SHA-512 file | none |
| FrankenPHP | GitHub php/frankenphp (native Windows since 1.12, 2026-03) | GitHub digest | Intel Mac |
| Java (Temurin) | api.adoptium.net/v3 | SHA-256 | none |
| Tomcat 9/10/11 | dlcdn.apache.org (archive.apache.org for older) | `.sha512` | needs Java |
| PostgreSQL 14–18 | GitHub theseus-rs/postgresql-binaries (all OS/arch, PostgreSQL licence) | check per release | replaces the EDB/own-CI idea |
| MongoDB | fastdl.mongodb.org, `downloads.mongodb.org/full.json` | SHA-256 | Linux builds are per distro |
| SQLite | sqlite.org CLI zips | SHA3-256 | none |
| ClickHouse | GitHub ClickHouse/ClickHouse | SHA-512 | no native Windows: the UI says WSL |
| Neo4j Community | dist.neo4j.org | SHA-256 | needs Java 21 |
| Meilisearch | GitHub meilisearch | GitHub digest | none |
| Typesense | dl.typesense.org | none published | no Windows |
| RustFS | GitHub rustfs/rustfs | GitHub digest | none |
| MinIO | LampRabit's CI build of the last release (`build-minio.sh`), every OS; upstream stopped Community binaries 2025-10 and archived the repo 2026-02 | built from a pinned source checksum | unmaintained upstream; revisit if it stops building (decided 2026-11-04) |
| OpenSearch / Elasticsearch | official tarballs/zips with a bundled JDK | SHA-512 | 1 GB+ RAM |
| RabbitMQ | GitHub rabbitmq-server (generic-unix, windows zip) | SHA-256 | needs Erlang; no official Linux Erlang binaries |
| Ollama | GitHub ollama | SHA-256 file | detect an existing install first |
| n8n | npm, run on LampRabit's Node | npm integrity | needs Node |
| Python | GitHub astral-sh/python-build-standalone (what uv uses) | SHA-256 | none |
| Go | go.dev/dl/?mode=json | SHA-256 | none |
| Rust | rustup (an existing `~/.cargo` is used as is) | rustup's own | none |
| Bun, Deno, uv | GitHub releases | SHA-256 | none |
| Maven / Gradle | Apache dist / services.gradle.org | SHA-512 / SHA-256 | need Java |
| conda | Miniforge | SHA-256 | heavy |

## Phase 11: service foundation and web servers

**Foundation first.** Each service used to be wired by hand in about 20 `match` arms (backend and frontend), which doesn't scale to the services planned here:
- `Versions` keeps its fields and gains a flattened map for new runtimes, so `settings.toml` loads unchanged. `Settings::version(id)`/`set_version` replace the per-field matches.
- `ServiceKind::def()` is one table per service (id, label, runtime, ports, log, data folder), so the getters stop being matches.
- `web_server` (Apache | Nginx | Both) becomes a set of enabled servers (`web_servers`), read from the old field when missing. The first enabled server keeps the main ports; every other server has its own pair in `Ports`.
- Runtimes declare what they need (Tomcat, Neo4j, Maven → Java; n8n → Node; RabbitMQ → Erlang), and the UI offers to install what's missing.
- A platform gap is a reason string per runtime, shown in the UI instead of hiding the runtime.

**Done (2026-11-01):** the `Versions` map, `ServiceKind::def()`, `web_servers` (with migration and a Settings UI: a switch per server and a primary picker), and Caddy end to end (catalog, Caddyfile, validate, reload, stop, health, ports, Sites/New project pickers). Then Java (Temurin, shims with JAVA_HOME), FrankenPHP (the Caddy renderer with `php_server`, conf.d ini through `PHP_INI_SCAN_DIR`, no LampRabit PHP pools) and Tomcat 9/10.1/11 (Java checked per line, `CATALINA_BASE` in etc/tomcat, one `<Host>` per Java project found by `sites::java_webapp`, proxied from the site's web server, restarted when the apps change). FrankenPHP worker mode per site done (2026-11-04: a switch in the project's settings, Octane's worker or index.php, "Restart workers" through the admin API). Still to do for the foundation: declared dependencies between runtimes, platform-gap reasons, and the frontend reading a backend catalog instead of its own tables.

**Web servers.** Caddy, FrankenPHP and Tomcat run next to Apache and Nginx: each site picks its server when more than one is enabled, and each extra server gets its own ports.
- **Caddy**: single binary from GitHub releases (all three OSes). LampRabit writes a Caddyfile: one site block per site, `php_fastcgi` to the existing PHP pools (so per-site PHP and the extension manager keep working), `reverse_proxy` for proxied sites/tools, `tls <cert> <key>` with LampRabit's CA and `auto_https off` so Caddy doesn't create a second CA. `caddy validate` before start (like `httpd -t`), `caddy reload` when sites change.
- **FrankenPHP**: the assessment from 7.3 still applies. It embeds its own PHP, so its version is shown read-only and its built-in extensions are listed; php.ini through `PHP_INI_SCAN_DIR` (Mailpit settings). Worker mode is an opt-in per-site toggle with a "restart workers" action. Intel Macs: no build, so the UI says so.
- Spike first on Windows (newest FrankenPHP platform): start, our certificates, extensions, `.user.ini`.
- **Java** (Temurin LTS lines 8, 11, 17, 21, 25) is a runtime first, because Tomcat needs it; phase 24 adds per-project versions and serving.
- **Tomcat 9/10/11**: LampRabit's own `CATALINA_BASE` in `etc/tomcat` (conf copied from the install, `server.xml` generated with one `<Host>` per Java site, webapps from the project folder) and `JAVA_HOME` from the active Java. HTTP only on its own port, with Apache/Nginx/Caddy proxying the `.test` domain (TLS stays in one place).
- Health probe, port conflicts, logs and the diagnostics report cover all three.

## Phase 12: projects (several domains, processes, Node.js)

**Done (2026-10-31, branch node-projects):**
- Node.js projects run with NVM's versions (per project, `.nvmrc`), any package manager (npm, pnpm, Yarn, Bun), from the template's `serve:` command or package.json. Projects start only from Sites, never with services.
- Port discovery (`ports.rs`): every port a project's processes listen on, whoever started them (LampRabit or your terminal), routes its domain; the lowest wins, or the one you pick; back to its files when nothing listens. PHP projects route only to a picked port. Web servers reload gracefully on a change (Nginx everywhere, Apache on Linux/macOS).
- Commands per project, from YAML (`commands:`) and discovery (package.json, composer.json, artisan, bin/console, bin/magento, please, drush, cake, spark), with output in the project and in Logs.
- Sites redesign: project list with live status, Overview (`project_facts`: versions, requirements, dependencies, .env, git), Commands, Settings.

**Done (2026-11-04):** extra domains per project, each pointing like the main one can: at the project's files or a folder in it, a port, one of its processes, or wherever the main domain goes (`SiteSettings.domains`; `sites::served` turns them into sites of their own, before the projects so Apache doesn't send them to `*.<project>`). Several processes per project (`SiteSettings.processes`: command, optional port as `{port}`/$PORT, environment, restart after a crash / always / never with at most 3 restarts in 5 minutes, start with the app), run by apps.rs as `<project>/<name>` with their own output. Their ports don't take over the main domain. Sites → Domains and Processes tabs, with Laravel/Symfony/Vite presets. The smoke test checks a project with both through every web server.

The groundwork for most later phases. A project stays a folder in `www/`; `SiteSettings` gains optional fields, so existing settings need no migration:

```toml
[sites.shop]
php = "8.3"
ssl = true

[[sites.shop.domains]]       # extra domains: to a folder, or to one of the processes
host = "admin.shop.test"
root = "admin/public"

[[sites.shop.domains]]
host = "api.shop.test"
process = "api"

[[sites.shop.processes]]     # long-running commands
name = "api"
command = "npm run dev"
port = 3000                  # or "auto": a free port, passed as $PORT
env = { NODE_ENV = "development" }
restart = "on-failure"
```

- Domains render through the existing vhost code (Apache, Nginx, Caddy), which already proxies for tools (`Site.proxy`). Before writing configs, check for duplicate hosts, ports in use and missing roots.
- Processes run under the supervisor with `procgroup`, with PATH set like the shims so `php`, `composer` and `node` are LampRabit's. Start/stop/restart one or all, restart policy, health (does the port answer), crash causes via `diagnose::explain_exit`.
- Node.js projects: detect `package.json` and the package manager from the lockfile (npm, pnpm, yarn, bun), suggest the `dev` script as the web process, use the `.nvmrc` version. A Node site is a process plus a proxied domain; the router knows nothing about Node.
- Stack profiles can include project processes.
- Project detail view (open from the card): domains, processes with their logs, quick actions.
- `lamprabit.toml` in the project folder (same shape, committable) only if sharing setups through the repo turns out to be needed; settings.toml would override it.

## Phase 13: logs redesign and dump catcher

- **Sources**: every service, every project process (phase 12; `LogLine.service` becomes an owned name like `shop/queue`), PHP errors, web-server access/error logs, and dumps.
- **Dump catcher**: PHP `dump()` and `dd()` appear in Logs as their own source, with the project, file:line, time and a collapsible tree of the value. Route: an `auto_prepend_file` in the generated php.ini sets Symfony VarDumper's server mode (`VAR_DUMPER_FORMAT=server`, `VAR_DUMPER_SERVER=127.0.0.1:<port>`), and LampRabit listens on that port. The dump-server payload is PHP-serialized, so spike the decoding: a small PHP receiver that re-emits JSON, or a LampRabit dumper class set as the handler. A setting decides whether dumps still show in the page too. Plain `var_dump` can't be intercepted; the UI says that.
- **Redesign**: source list (services, projects → processes, dumps) next to the stream; filters for level, source, time range and text (regex optional); highlighted matches; pause/follow; virtualised list so large logs don't block the UI; click a line for context (surrounding lines, full text, copy); export the filtered view; per-source clear.
- **Retention**: size cap per log file and total, configurable in Settings; old files rotated.
- Errors that need action (fatal, failed start) are highlighted; warnings aren't treated as failures.

## Phase 14: movable tab bar

The page tab bar (`BottomNav`) can sit at the bottom (default), top, left or right: a setting, plus dragging the bar to an edge with a drop preview. Vertical positions stack the tabs and move the page content aside. The position is saved per device, `Escape`/a reset action puts it back, and it never ends up off-screen. Check both themes, Motion settings, and the full-width wrapper pitfall (`pointer-events-none`).

## Phase 15: more databases

**Done (2026-11-01): PostgreSQL 14–18.** Newest build per major from theseus-rs, data per major in `data/postgres-<major>` (`initdb -U postgres -A trust -E UTF8 --no-locale`), only on 127.0.0.1 with no Unix socket, stopped with `pg_ctl stop -m fast`, an SSLRequest health probe, a `root` superuser added after each start (frameworks default to root), shims `psql`/`pg_dump`/`pg_dumpall`/`pg_restore`/`createdb`/`dropdb` with PGHOST/PGPORT/PGUSER, `pgsql`/`pdo_pgsql` in PHP (Windows defaults, and the Linux/macOS builds' extension list), Adminer lists it and the Overview opens it there. Then (same day): templates declare `engines:` (Laravel and Symfony: MariaDB, MySQL or PostgreSQL; the others MySQL-family for now) and the wizard picks the server; backups (`backups.rs`, `<root>/backups/<engine>/`, kept on uninstall) for MariaDB/MySQL (`mariadb-dump`/`mysqldump`) and PostgreSQL (`pg_dump -Fc`/`pg_restore --clean`), with restore into any database name, in each database's details; phpPgAdmin (ReimuHakurei's PHP 8 fork, a tool like phpMyAdmin) and pgAdmin 4 (PyPI wheel in its own venv on LampRabit's Python, desktop mode as a service proxied at pgadmin.test); Python itself (python-build-standalone, `python`/`pip` shims); MongoDB 7–9 (MongoDB's `full.json`, Linux build picked from /etc/os-release, data per major, stopped through mongosh) and mongosh.
Later the same day: MongoDB Database Tools (mongodump/mongorestore) and MongoDB backups; MongoDB Compass (opened connected to LampRabit's MongoDB); pgAdmin registers LampRabit's PostgreSQL once (`setup.py load-servers`); SQLite (sqlite3 shell, SHA3-checked; Laravel projects can use it with nothing installed); ClickHouse (Linux/macOS, generated config.xml/users.xml, native port 19000 since 9000 is PHP's, console at /play); Neo4j Community (Java 21+, NEO4J_CONF in etc/neo4j, Browser at neo4j.test). The Services page offers related services with one-click installs (`src/lib/related.ts`).
Drupal, CakePHP and CodeIgniter on PostgreSQL done (2026-11-04, `{{db.pdo}}`; Drupal gets pg_trgm). WordPress and Magento stay MySQL-family (upstream supports nothing else). SQLite backups done (2026-11-04): the .sqlite/.db files in projects, through sqlite3 `.backup` when installed (WAL-safe), a plain copy otherwise.

- **PostgreSQL**: theseus-rs/postgresql-binaries on every OS (built from source in CI, all six OS/arch targets; replaces the EnterpriseDB-plus-own-CI idea, decided 2026-11-01). Data folder per major version (`data/postgres-17`), `initdb` on first start, port 5432, `psql`/`pg_dump` shims, `client` config like MariaDB's, Adminer lists it, new projects can choose it (`.env` lines, `pdo_pgsql` enabled).
- **MongoDB**: official tarballs (all three OSes, check arch), `mongosh` downloaded with it, a `mongodb` PHP extension through the extension manager, connection string on the Overview.
- **ClickHouse**: the single binary (`clickhouse server` with a generated config, data in `data/clickhouse`, HTTP 8123 and native 9000 moved to free ports). No native Windows build, so the Windows UI explains WSL instead.
- **Neo4j Community**: the official archive on the active Java 21+, data in `data/neo4j`, Browser UI at `neo4j.test`, Bolt port on the Overview.
- **SQLite**: no server. New projects can choose it (create `database/database.sqlite`, write `.env`); presets offer it where the framework supports it.
- **Backups**: dump and restore per database (`mysqldump`/`mariadb-dump`, `pg_dump`, `mongodump`, file copy for SQLite) into `backups/`, with a restore picker.
- No in-app database browser (decided 2026-10-29).

## Phase 16: more services

**Done (2026-11-01), batch 1:** Meilisearch (Community builds, development mode), Typesense (dl.typesense.org, Linux/macOS, fixed local API key), RustFS (S3 on 9100, console 9101; `Settings.storage` puts its buckets anywhere, outside the LampRabit folder too) and Ollama (its own builds incl. Linux .tar.zst through `ruzstd`; models in data/ollama; an Ollama installed elsewhere is pointed out).

**Batch 2 (same day):** Elasticsearch 8/9 and OpenSearch 2/3 (bundled Java, ES_PATH_CONF / OPENSEARCH_PATH_CONF in etc/, single node, security off, 512 MB heap; OpenSearch has no macOS build), n8n (npm install into bin/n8n on NVM's Node 24+, editor at n8n.test), RabbitMQ 4 with its management console (Erlang: official zip on Windows, the system's elsewhere), MinIO's last release built from source in `binaries.yml` (`build-minio.sh`, Go 1.24, AGPL source linked, labelled unmaintained). Phase 16 is complete; MinIO 2025.10.15 built and published for all five targets on 2026-11-05 (run 42081376875).

Same standards as existing runtimes: catalog per OS/arch with checksums, data under `data/`, health probe, logs, ports in Settings, connection details on the Overview, `.test` domain for web UIs, data kept on upgrade/remove. Resource needs (RAM, Java, Erlang) shown before installing. Rough order, cheapest first:
1. Meilisearch, Typesense (single binaries; Typesense has no Windows build)
2. RustFS and MinIO, both S3-compatible (decided 2026-11-01: offer both). RustFS from its GitHub releases, console at `rustfs.test`. MinIO stopped publishing binaries in 2025 and was archived in 2026, so it comes from LampRabit's CI build of its last release (a Go build in `binaries.yml`, AGPL source linked), labelled "no longer maintained upstream, no security fixes" in the UI. Each keeps its objects in `data/<name>` by default; a separate storage-folder setting per service can put them anywhere (another drive, a NAS), independent of the LampRabit folder. Moving it offers to move the existing objects, and an outside folder is never deleted on uninstall.
3. n8n (an npm package run on LampRabit's Node; data in `data/n8n`; needs Node installed first)
4. OpenSearch / Elasticsearch (bundled JDK, 1 GB+ RAM)
5. RabbitMQ (needs Erlang; Windows support to be checked)
6. Ollama (detect an existing install first; often installed by the user)

## Phase 17: templates and more frameworks

Done (2026-10-30): YAML templates (`src-tauri/templates/`, user ones in `<root>/templates/`, reference in docs/templates.md) with variables, optional steps, `if:` conditions, requirement checks, risky-step approval for user templates, and the steps `composer`, `php`, `run`, `download`, `write`, `append`, `copy`, `env`, `replace`, `create_database`, `git_init`. Built in: the five presets plus CakePHP, CodeIgniter, Drupal and Statamic, each verified by creating a project in WSL. The site records `template = "id@version"`. Since then: Magento, and Node templates (Vite, Next.js, Nuxt, Astro, Express, NestJS) whose variables mirror each CLI's own questions (labelled options, conditions, flags, multi-select). Still to do: extra domains, the template creation wizard, recording a setup as a template.

- The hard-coded `Preset` enum becomes declarative, versioned templates (built in plus a user folder): variables with defaults and validation, prerequisites (PHP ≥, extensions, Node, a database), steps (`composer create-project`, commands, `.env` keys, create database, domains/processes from phase 12, git init), live output, cleanup on failure. The five current presets are ported first, behaving the same.
- New: CakePHP, CodeIgniter 4, Drupal, Statamic; Vite (React/Vue/Svelte), Next.js, Express API; Laravel + Vite. Magento last, with a requirements check (Adobe auth keys, OpenSearch from phase 16, memory).
- The project records which template and version created it.

## Phase 18: onboarding

First-run wizard: what LampRabit does, a starter (PHP, Laravel, WordPress, Node.js, full stack, custom), each component with download size and platform notes, install with progress and retry, optionally create a first project. Already-installed runtimes are detected and skipped. Stack profiles gain export/import. Can be re-opened from Settings.

## Phase 19: project doctor

Built on `diagnose.rs`. Per project: PHP version vs `composer.json`, missing extensions, `vendor/` or `node_modules` missing or out of date with the lockfile, missing `.env` or unreachable database, certificate expiry, hosts entry, port conflicts, invalid vhost/domain config. Each issue: what, why, affected project/service, the fix, whether it can be applied automatically, and a preview before applying. Nothing is upgraded, deleted or overwritten without confirmation.

## Phase 20: headless core and the `lamprabit` CLI

- One backend process owns the services. The app can run without a window (tray only, `--background`); the window and the CLI are both clients of it, so they always show the same state.
- `lamprabit` (installed as a shim like `php`) connects over a local socket / named pipe with a token in the app data folder. If no backend is running, it starts one in the background. Opening the app later attaches to the same backend (the single-instance plugin already routes a second launch).
- Commands: `status`, `service start|stop|restart`, `project list|start|stop|open`, `runtime list|install|use|remove`, `logs [--follow] [--source]`, `doctor`, `profile`, `config`. `--json` everywhere, stable exit codes, `--yes`/`--dry-run` for destructive actions.
- Commands reuse the Tauri command code paths (no second implementation): split the bodies in `lib.rs` into functions both call.

## Phase 21: MCP server

`lamprabit mcp` (stdio) on top of the phase 20 backend: list projects, services and their health, read filtered logs and dumps, run the doctor, start/stop, create a project from a template, git status. Read-only tools by default; tools that change things are marked so the client asks first; credentials and keys are never returned, only connection metadata.

## Phase 22: Git

Decided 2026-11-01: Git installed by LampRabit plus a Git panel per project; no local Git server (Forgejo). Windows: MinGit/portable Git for Windows from GitHub (SHA-256) under `bin/git`. Linux/macOS: the system git. When it's missing, LampRabit installs it the easiest way the OS offers: `xcode-select --install` on macOS (Apple's own install dialog), and on Linux the distro's package (`apt`/`dnf`/`pacman`/`zypper`, detected) through pkexec, showing the command first. If neither works, it links the official download page. A `git` shim like the others.

Through the `git` CLI, so the user's credential helpers and SSH keys work: status, stage/unstage, commit, branches (create, switch, rename, delete), history with diffs, fetch/pull/push, merge conflicts listed, clone into `www/`, remotes. Per-repository identity editor (`git config --local user.name/email`), never changing global config unless asked.

## Phase 23: dependency manager

Composer and npm/pnpm/yarn/bun panels per project: installed and outdated packages, install/update/require/remove with live output, run with the project's PHP and Node versions. No upgrades without consent; lockfile kept consistent.

## Phase 24: languages

**Done (2026-11-01):** Python as a runtime; Python projects (`sites::Language::Python`, Django/Flask/FastAPI detected) and Java projects serve through their template's dev server like Node; templates Django, Flask, FastAPI, Spring Boot (Spring Initializr + Maven wrapper) and a plain Java web app for Tomcat; `requires.python`/`requires.java`; JAVA_HOME for steps and dev servers.
Then Go (go.dev, the two supported lines) and Rust (rustup-init into bin/rust with RUSTUP_HOME/CARGO_HOME there; Windows uses the GNU host so no Visual Studio is needed), `Language::Go`/`Rust` with Gin/Echo/Fiber/Chi and Axum/Actix/Rocket detection, templates Go (net/http), Gin and Axum. Per-project versions (`langver.rs`): `.python-version`, `.java-version`/`.sdkmanrc`, `go.mod`'s toolchain/go line and `rust-toolchain.toml` pick the installed version (exact, else newest with the same major.minor); the python/java/go shims choose by folder like PHP's, rewritten when a version file changes (watch loop); Sites shows the version and its file, with a picker that writes `.python-version`/`.java-version`. rustup handles Rust's file itself.

Python, Go, Rust and Java, each as a runtime with versions side by side, like PHP. Decided 2026-11-01: LampRabit installs its own copies under `bin/` (pyenv, sdkman and the like aren't used):
- Sources in the table above. Rust goes through rustup with `RUSTUP_HOME`/`CARGO_HOME` under `bin/rust`, so it stays inside LampRabit too.
- Per-project version from the usual file (`.python-version`, `go.mod`'s `toolchain`, `rust-toolchain.toml`, `.java-version`), shown and changeable in the project's Settings. Shims (`python`, `go`, `java`) pick it like the PHP shim.
- `sites::Language` gains `Python`, `Go`, `Rust` and `Java`. Serving needs nothing new, since port discovery routes any process by its working folder. `apps::suggest` learns `manage.py runserver`, `uvicorn`/`flask run`, `go run .`, `cargo run`, `mvn spring-boot:run`/`gradle bootRun`. WAR projects can deploy to Tomcat instead.
- Templates: Django, Flask, FastAPI, Go (net/http, Gin), Rust (Axum), Spring Boot (start.spring.io), Quarkus.

## Phase 25: package managers

Absorbs phase 23. npm comes with Node, pip with Python and Cargo with Rust; pnpm and Yarn come through corepack; Bun, Deno, uv, Maven, Gradle and conda (Miniforge) are runtimes. Command discovery learns `pyproject.toml`, `requirements.txt`, `pom.xml`, `build.gradle(.kts)`, `Cargo.toml` and `deno.json`. Then the dependency panels: installed and outdated packages, add/update/remove with live output, the project's own language versions, nothing upgraded without consent.

## Phase 26: movable environment folder

**Done (2026-11-01):** `relocate.rs` and Settings → LampRabit folder → Move. Checks (inside/around the folder, clashing folders, write access, system folders, free space, FAT/exFAT, network drives, Linux noexec) through sysinfo's disk list; programs using the folder (exe or working directory inside it, from sysinfo rather than the Windows Restart Manager) with Stop it; rename on the same drive, copy-check-delete across drives, rolled back on failure; PATH and shims switched, Python venvs (pgAdmin's, projects' .venv) repointed. Left: choosing the folder on first run (onboarding).

Decided 2026-11-01: the LampRabit folder (bin, etc, www, data, logs…) can live anywhere the OS allows, and changing it moves everything.
- Chosen on first run (onboarding) and changeable in Settings → Environment folder → Move.
- Move: stop services and project processes, check the target, move the folders LampRabit owns plus `www/`, `data/`, `templates/` (a rename on the same drive, copy-then-verify-then-delete across drives, with progress and resume after a failure), then regenerate configs, rewrite shims, update PATH, the terminal profile and anything else that stores the path (MariaDB/MySQL data paths, Caddy storage, the CA trust stays valid since the certificate is unchanged).
- A file in use: list which program holds it (Windows Restart Manager, `lsof`/`/proc` elsewhere) with "Stop it" (ends that process) and "Retry"; nothing is half-moved, so a cancelled move leaves the old folder working.
- Refuses targets that can't work, saying why: no write access, system folders, a drive mounted `noexec` (Linux/macOS external drives often are), FAT/exFAT (no permissions or symlinks; databases fail), network drives, not enough free space.
- Folders outside the root (MinIO/RustFS storage, see phase 16) aren't moved.

## Phase 9: Linux parity and developer dashboard

### 9.1 Fixes
- Single instance: a second launch brings the running window forward (tauri-plugin-single-instance).
- Composer is a runtime on the Services page: install, switch and remove versions (current and 2.2 LTS); presets install it when needed.
- Linux: the AppImage aborted on systems without libGLESv2 (fixed with `WEBKIT_DISABLE_DMABUF_RENDERER=1`); Apache and Nginx daemonized and went untracked (now run in the foreground); services left behind by a crash are stopped on the next launch.

### 9.2 Linux runtimes (decided: LampRabit's own CI builds)
Apache, Nginx and PHP are built by `.github/workflows/binaries.yml` from `build/linux/` and published to the `binaries` release with a checksummed manifest (details in RELEASING.md). Distro packages were considered and rejected: one PHP version only, per-distro paths and service conflicts. Ports 80/443 need one administrator prompt (`/etc/sysctl.d/50-lamprabit.conf`). HTTPS trust covers the system store plus Chrome/Firefox NSS databases (`certutil`). Open terminal works with GNOME's terminals; the MariaDB client button opens a terminal.

### 9.3 MySQL
Oracle MySQL next to MariaDB (port 3307 by default): 8.4 LTS, the newest LTS and the Innovation release, from MySQL's CDN (Linux: the minimal build), each release line with its own data folder.

### 9.4 Dashboard redesign
- Service list with selection and bulk Start/Stop; each row shows status, port, PID, uptime, port conflicts and problems, CPU and RAM sparklines (PHP workers' usage on the web-server rows) and a click-to-copy connection string.
- **Stack profiles**: named sets of services (saved in settings). Starting a profile starts its services and stops the others; the title bar and Shift+S follow the active profile.
- "Connect your app" panel: every service as a URL or as ready-to-paste `.env` lines, plus "Copy everything as .env".
- Keyboard: `?` lists every shortcut; 1–5 switch pages; J/K or arrows move between services, Space selects, Enter starts/stops, S/X/R/O/C/E/P/L act on the selection or focused service. Dialogs trap focus and restore it; menus use arrow keys.

### 9.5 Databases, Redis and page redesigns
- Default database setting (MariaDB or MySQL); phpMyAdmin and Adminer list every installed database, the default first (fixes logins failing when MariaDB wasn't the running server).
- Redis per platform: Garnet on Windows, with `redis-cli` from the community redis-windows build; real Redis (LampRabit's builds, 8.10 and 7.2) with its own `redis-cli` on Linux and macOS. phpRedisAdmin for browsing Redis data, served like phpMyAdmin. (A custom redis-cli and in-app browser were tried first and removed.) Garnet was replaced by real Redis in 10.1.
- Sites: project cards show framework, git branch, database (opens in phpMyAdmin), Node version, last change, PHP/server/HTTPS controls and per-project sharing; search, framework filter and sort. Port sharing moved to Services → Sharing.
- Services: grouped (web and PHP, databases, developer tools, sharing); the version in use is a large tile, other installed versions are tiles, downloads a list. Node.js uses the same layout.
- Settings: one column of compact sections with a section index; duplicate Mailpit .env block removed (it's on the Overview).

## Phase 6: feature set

Platform rule for everything below: a setting or tool is shown only on platforms where it works; where it doesn't, say why instead of hiding it silently.

Order: 6.1 → 6.2 → 6.3 → 6.4 → 6.5 (reliability first; per-project PHP and presets build on the web-server work).

### 6.1 Service reliability ✅ (plus custom title bar)
- **Health checks**: beyond "process alive": Apache answers an HTTP probe, MariaDB answers `mariadb-admin ping`, configs pass `httpd -t` before start. States: running / stopped / degraded / failed, each with a plain-language cause and fix (known patterns: port in use, missing VC++ runtime, bad config line, InnoDB recovery).
- **Port conflicts**: identify the owning process (Windows: TCP table → PID → name; macOS/Linux: `lsof`), explain common culprits (IIS/`http.sys` as PID 4, Skype, another XAMPP/Laragon), offer "use another port" or "stop that process" when it's safe.
- **Activity logs**: existing System stream plus filters by severity and time range, and export (save dialog → `.log`/`.txt`).
- **Diagnostics report**: OS and app versions, runtimes, ports and owners, config test output, hosts/CA status, recent errors; copy or save as a file, with user paths optionally redacted.

### 6.2 Web servers ✅
- **Apache, Nginx or both**: Nginx from nginx.org (Windows zips; macOS/Linux via own builds or package manager). PHP for Nginx runs as FastCGI (`php-cgi` pool on Windows, `php-fpm` elsewhere). With both enabled, each site picks its server; the second server gets its own ports (default 8080/8443) and the UI blocks overlapping ports before start.

### 6.3 Projects ✅
- **Create project** wizard: name → folder in `www/` → preset → options (database, HTTPS) → done, with live progress.
- **Presets**: Blank PHP, Laravel, WordPress, Symfony, static. A preset can create a database and user, pick the PHP version, and enable tools (Mailpit, Redis). Needs Composer (downloaded `composer.phar`) and WP-CLI or the WordPress zip.
- **Per-project PHP version** (decided: yes): PHP runs as FastCGI per installed version (`php-cgi` pool on Windows, `php-fpm` elsewhere) instead of a single `mod_php`; each site picks its version, default is the global active one. Shared with Nginx support in 6.2, so 6.2 lands first.

### 6.4 Tools and tunnels ✅
- **Node.js via NVM** (decided): not installed by default. When the user asks, LampRabit installs NVM and from then on manages Node versions through it (list, install, use, `.nvmrc` per project).
  - Windows: nvm-windows. Its `nvm use` creates a symlink, which needs admin rights or Developer Mode; the UI says so before installing.
  - macOS/Linux: nvm's official install script into `~/.nvm`; commands run through a login shell that sources `nvm.sh`.
  - An existing NVM install is detected and used instead of installing a second copy.
- **Tunnels: Ngrok + Cloudflare** (decided): app-level (expose `localhost`) and per-project (expose `myapp.test` with a host-header rewrite, or a manual port).
  - Ngrok (default): binary downloaded on demand, auth token stored in the OS keychain, free-plan limits explained in the UI.
  - Cloudflare quick tunnels (`cloudflared`): no account needed, random `trycloudflare.com` URL.
- **Redis / Memcached**: no official Windows builds for either. Windows: Redis-compatible alternative (Garnet) or Memurai, or "not available" with explanation; macOS/Linux: manage a package-manager install (brew/apt) or a downloaded build. PHP extensions (`redis`, `memcached`) offered only where builds exist for the active PHP. (Superseded in 10.1: real Redis and downloadable Memcached on every OS.)

### 6.5 Updates ✅
- **App updates**: Tauri updater with signed releases on GitHub; check on start (opt-out) and from Settings. Depends on the release pipeline in phase 8.
- **Runtime updates**: badge when a newer patch of an installed Apache/PHP/MariaDB is available.

## Phase 7: comfort and PHP tooling

### 7.1 Comfort ✅
- Tray menu (start/stop all, open sites, quit), start with the OS, optionally start services on launch.
- "Open terminal" with PHP, Composer, MariaDB and Node on PATH; "Open in editor" for projects.
- Microsoft Visual C++ runtime check on Windows, with a link to (or download of) the redistributable.
- MariaDB data directory per major version, or an automatic `mariadb-upgrade` when moving up.

### 7.2 PHP extension manager ✅
Browse, enable, disable and add PHP extensions per installed PHP version, with compatibility shown for each.

- **Bundled extensions**: list what each PHP build ships (Windows: `ext/php_*.dll`; elsewhere: `php -m` plus shared objects in the extension dir). Toggle them on or off per version; the choice is stored in settings and written into that version's generated php.ini, then the PHP pool restarts.
- **Extra extensions** (PECL and vendor builds): Xdebug, Redis, APCu, Imagick, MongoDB, PCOV, Memcached and similar.
  - Windows: download prebuilt DLLs matching the PHP minor version, thread safety (LampRabit's PHP builds are thread-safe) and compiler (vs16 / vs17) from downloads.php.net/~windows/pecl and xdebug.org. Show "available for PHP 8.3, not yet for 8.5" style compatibility from what actually exists for download.
  - macOS / Linux: depends on how PHP itself is provided there (static builds can't load shared extensions); show "built in" / "not available on this platform" accordingly.
- Extensions with extra requirements are flagged (Imagick needs ImageMagick DLLs; Xdebug gets a ready-made `xdebug.mode=debug` and IDE port setting).
- Status per extension: built in, enabled, disabled, downloadable, unavailable (with the reason).

### 7.3 FrankenPHP: brought back in phase 11

Decision (2026-10-24): not planned. Reversed on 2026-10-29: FrankenPHP and Caddy are phase 11. The assessment below still applies.

#### Assessment
FrankenPHP is a PHP application server built on Caddy: the web server and PHP run in one process, with automatic HTTPS, HTTP/2 and HTTP/3, and a "worker mode" that keeps the application booted between requests (Laravel Octane, Symfony Runtime).

**Availability (checked against v1.13.3 releases):** Linux x86_64 and aarch64, macOS Apple Silicon (no Intel Mac build), and a Windows x86_64 zip. All three platforms can be offered; Intel Macs get "not available" with the reason.

**How it fits LampRabit**
- It is a third web server, not a PHP version: each FrankenPHP release embeds its own PHP, so it doesn't use the PHP pools, php.ini per version, or the extension manager. Its PHP version is shown read-only on the site card, and its bundled extensions are listed (static builds include the common ones).
- Per-site choice: Settings → Web server gains "FrankenPHP" as an option next to Apache and Nginx; sites can pick it like they pick Nginx today. It gets its own ports when another server holds 80/443.
- Config: LampRabit generates a Caddyfile with one `php_server` block per site, using LampRabit's own certificates (`tls <cert> <key>`) so the existing trusted CA keeps working, and Caddy's automatic HTTPS turned off to avoid a second CA.
- Worker mode as a per-site toggle (off by default) for projects that support it; a restart button on the site card reloads workers after code changes.
- Mail: `sendmail_path`/SMTP settings through FrankenPHP's php.ini (`PHP_INI_SCAN_DIR`) so Mailpit keeps catching mail.
- Logs, health checks (HTTP probe), port-conflict detection and the diagnostics report extend to it like the other servers.

**Risks / open questions**
- Windows build is the newest platform; needs a spike (start, PHP extensions loaded, our certificates, `.user.ini`) before committing.
- Extensions are fixed at build time; projects needing an extension that isn't in the build must use Apache/Nginx.
- Worker mode changes app behaviour (state persists between requests); keep it opt-in with a clear explanation.

**Recommendation at the time:** an optional third server after the extension manager, starting with a spike on Windows and Linux.

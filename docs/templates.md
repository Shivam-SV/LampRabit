# Project templates

**New project** creates a folder in `www/` by running a *template*: a YAML file listing what to download, which commands to run and which files to write. LampRabit ships templates for Blank PHP, Laravel, Symfony, WordPress, CakePHP, CodeIgniter, Drupal, Statamic, Magento, Vite, Next.js, Nuxt, Astro, Express, NestJS and a static site. You can add your own.

## Your own templates

Put `.yaml` (or `.yml`) files in the **`templates`** folder inside your environment folder (`C:\lampRabit\templates` or `~/lampRabit/templates`). **New project → Your templates** opens it, and creates it the first time with a commented example, `example.yaml.txt`. Rename that file to `.yaml` to try it.

- Templates are read every time **New project** opens, so changes show up straight away.
- A template whose `id` matches a built-in one replaces it.
- A file that can't be read is listed in the wizard with the reason (a typo, an unknown placeholder, a missing field), and the other templates still work.
- Uninstalling LampRabit keeps the `templates` folder, like `www` and `data`.

### Safety

Templates run commands on your computer. Built-in templates run without asking. For your own templates, LampRabit lists every step that looks risky before running anything, and you must tick a box to approve them. Risky steps include:

- deleting files recursively (`rm -rf`, `rmdir /s`, `Remove-Item`);
- running as administrator (`sudo`, `runas`);
- piping downloads into a shell (`curl … | sh`, `iex`);
- touching system or home folders, or reaching outside the project (`..`);
- editing the registry;
- downloading a file without a checksum, or over plain HTTP.

Everything else runs normally. File steps (`write`, `copy`, `env`…) can only touch paths inside the new project.

## A complete example

```yaml
schema: 1
id: laravel-breeze
name: Laravel + Breeze
description: Laravel with Breeze authentication and a built frontend.
version: 1
icon: flame
color: red
duration: Composer and npm · a few minutes

requires:
  php: "8.2"
  extensions: [mbstring, openssl, pdo_mysql]

database: optional
engines: [mariadb, mysql, postgres]

variables:
  - id: stack
    label: Breeze stack
    type: choice
    options: [blade, react, vue]
    default: blade

steps:
  - create_database: {}
  - name: Creating the Laravel project
    cwd: www
    composer: [create-project, laravel/laravel, "{{project}}", --prefer-dist, --no-interaction]
  - name: Connecting the database
    if: database
    env:
      values:
        DB_CONNECTION: "{{db.engine}}"
        DB_HOST: "{{db.host}}"
        DB_PORT: "{{db.port}}"
        DB_DATABASE: "{{database}}"
        DB_USERNAME: "{{db.user}}"
        DB_PASSWORD: "{{db.password}}"
  - name: Installing Breeze
    composer: [require, laravel/breeze, --dev]
  - php: [artisan, "breeze:install", "{{vars.stack}}", --no-interaction]
  - if: database
    php: [artisan, migrate, --force]
  - id: frontend
    optional: true
    label: Install and build the frontend with npm
    needs: [node]
    run: npm install && npm run build

done: "Ready at {{url}}. Register an account to try Breeze."
```

## Reference

### Top level

| Key | Required | Meaning |
|---|---|---|
| `schema` | yes | Template format version. Must be `1`. |
| `id` | yes | Unique name: letters, numbers, `-` and `_`. Recorded with each project (`id@version`). |
| `name` | yes | Shown on the template card. |
| `description` | | One line under the name. |
| `version` | | The template's own version (default 1). Bump it when you change the steps. |
| `icon` | | `code`, `file`, `flame`, `flame-kindling`, `boxes`, `newspaper`, `cake`, `droplet`, `zap`, `layers`, `box`. |
| `color` | | `red`, `orange`, `amber`, `sky`, `blue`, `indigo`, `pink`, `rose`, `teal`, `zinc`. |
| `duration` | | A hint such as `Composer · about a minute`. |
| `requires.php` | | Lowest PHP version (`"8.2"`) or `any`. Leave it out for projects without PHP; the wizard then hides the PHP choice. |
| `requires.extensions` | | PHP extensions that must be loaded. Checked with `php -m` before anything is created; the error says to turn them on in Services → PHP → Extensions. |
| `requires.python` | | Lowest Python version (`"3.10"`) for Python projects. LampRabit's Python must be installed (checked before anything is created); `python` and `pip` are on PATH in steps, and `{{venv.python}}` is a virtual environment's Python. |
| `requires.java` | | Lowest Java version (`"17"`) for Java projects. LampRabit's Java must be installed; steps and the dev server get `JAVA_HOME`. |
| `requires.go`, `requires.rust` | | Lowest Go (`"1.22"`) or Rust (`"1.80"`) version. LampRabit's own must be installed; `go`, `cargo` and `rustc` are on PATH in steps and the dev server (Rust with LampRabit's rustup homes). |
| `requires.node` | | `true` for JavaScript projects: the wizard asks for a Node.js version (from NVM) and a package manager, and the `{{pm.*}}` placeholders work. |
| `database` | | `none` (default), `optional` (a switch, on when a database server is installed) or `required`. |
| `engines` | | The database servers the project can be set up for: `mariadb`, `mysql`, `postgres`, `sqlite` (a file: `{{database}}` is then the path `database/database.sqlite`, and nothing is created up front). Default `[mariadb, mysql]`. With more than one installed, the wizard lets the user pick; write the config with the `db.*` placeholders so it works for each. |
| `variables` | | Extra fields in the wizard (below). |
| `steps` | yes | What to do, in order (below). |
| `serve` | | How the project's domain is served (below). Without it, the web server serves the files, with PHP. |
| `commands` | | What the project's Commands tab offers (below). |
| `done` | | Shown when the project is ready; placeholders work. |

### Variables

```yaml
variables:
  - id: stack            # used as {{vars.stack}}
    label: Breeze stack
    type: choice         # text (default), choice or bool ("true"/"false")
    options: [blade, react, vue]
    default: blade
    help: Shown under the field.
    required: true       # Create stays disabled while it's empty
  - id: api_key
    label: API key
    if: vars.stack == react   # shown (and filled in) only while this holds; empty otherwise
```

Choices can carry labels and follow earlier answers, and a variable can turn into a CLI flag, so a template asks the same questions as the framework's own installer:

```yaml
variables:
  - id: framework
    label: Framework
    type: choice
    options:
      - { value: react, label: React }
      - { value: vue, label: Vue }
  - id: linter
    label: Linter
    type: choice
    if: vars.framework == react          # the whole field
    options:
      - { value: --no-eslint, label: Oxlint }
      - { value: --eslint, label: ESLint, if: vars.framework == react }   # one option
  - id: typescript
    label: TypeScript
    type: bool
    default: "true"
    on: "-ts"                            # {{vars.typescript}} when on
    off: ""                              # ...and when off
  - id: integrations
    label: Integrations
    type: multi                          # toggle chips; several values
    flag: "--add {value}"                # each picked value becomes "--add react"
    separator: " "                       # between them (default ",")
    options: [react, tailwind, mdx]
```

`{{vars.x}}` is the rendered value (with `on`/`off`/`flag` applied); conditions (`if:`) always compare the raw answer. A choice whose picked option disappears moves to the first one left.

Optional steps with an `if:` show their checkbox only while the condition holds, so a template can offer one step per choice (Magento's two-factor switch differs between Mage-OS and Adobe's edition).

### Steps

Each step has exactly one action, plus any of these common keys:

| Key | Meaning |
|---|---|
| `name` | The progress heading. Without one, LampRabit describes the action. |
| `if` | Runs only when this holds: `database`, `!database`, `vars.stack == react`, `vars.stack != blade`. Empty, `false` and `none` count as false. |
| `optional` | Shows a checkbox in the wizard. Use `label` for its text and `default: false` to start unticked (default `true`). |
| `id` | Names the step (needed for optional steps). Defaults to its position. |
| `needs` | Tools the step needs on PATH: `node` (npm, npx) or `git`. Checked before anything is created, but only when the step will run. |
| `cwd` | Working folder: `www`, or a folder inside the project. Defaults to the project folder. `composer create-project` runs in `www`. |

The actions:

| Action | Example | What it does |
|---|---|---|
| `composer` | `composer: [require, laravel/breeze, --dev]` | Runs Composer with the project's PHP and LampRabit's php.ini. Composer is installed the first time it's needed. |
| `php` | `php: [artisan, migrate, --force]` | Runs the project's PHP. |
| `run` | `run: npm install && npm run build` | A shell command: `cmd /C` on Windows, `sh -c` elsewhere. The project's PHP and LampRabit's commands (`php`, `composer`, `mysql`, `redis-cli`) come first on PATH. |
| `download` | see below | Downloads a file, checks its checksum, and unpacks or saves it. |
| `write` | `write: { path: index.php, content: "…" }` | Creates a file (and its folders). |
| `append` | `append: { path: settings.php, content: "…" }` | Adds to the end of a file. |
| `copy` | `copy: { from: .env.example, to: .env }` | Copies a file inside the project. |
| `env` | see below | Sets keys in a `.env`-style file, adding missing ones. |
| `replace` | see below | Replaces text in a file. |
| `create_database` | `create_database: {}` | Creates the database named in the wizard, starting the database server if needed. Skipped when no database was chosen. Put it first: a name that's taken then stops everything before any long download. |
| `git_init` | `git_init: { commit: First commit }` | `git init`, plus a first commit when Git knows your name and email. Skipped, with a note, when Git isn't installed. |

`download`:

```yaml
- download:
    url: https://wordpress.org/latest.zip
    sha1_url: https://wordpress.org/latest.zip.sha1   # or sha256_url, or sha256: <hex>
    extract: true          # .zip, .tar.gz or .tgz, unpacked into the project
    strip: wordpress       # take the contents of this folder in the archive
    # to: vendor/tool.phar # without extract: where to save it (default: the file name)
```

`env` finds keys even when the file ships them commented out (`# DB_HOST=…`), and keeps a file's `KEY = value` spacing (CodeIgniter):

```yaml
- env:
    file: .env.local       # default .env
    values:
      APP_URL: "{{url}}"
      MAIL_PORT: "{{mail.port}}"
```

`replace` renders the new text separately for each occurrence, so `{{random:64}}` gives every match its own secret (WordPress's keys and salts). If the text isn't found, the step notes it in the output and carries on, because upstream files change between releases.

```yaml
- replace:
    file: wp-config.php
    values:
      database_name_here: "{{database}}"
      put your unique phrase here: "{{random:64}}"
```

### Placeholders

Write `{{name}}` anywhere a step takes text. Unknown names are reported when the template loads, not halfway through a project.

| Placeholder | Value |
|---|---|
| `{{project}}` | The folder name |
| `{{domain}}`, `{{url}}` | `shop.test`, `https://shop.test` (no trailing slash) |
| `{{www}}`, `{{dir}}` | The www folder, the project folder |
| `{{php_version}}` | The PHP version the project uses |
| `{{database}}` | The database name from the wizard (empty when none) |
| `{{db.engine}}` | `mariadb`, `mysql` or `pgsql` (Laravel's connection names) for the server chosen in the wizard |
| `{{db.host}}`, `{{db.port}}` | `127.0.0.1` and the server's port |
| `{{db.user}}`, `{{db.password}}` | `root` and an empty password |
| `{{db.server_version}}` | `11.8.9-MariaDB`, `8.4.13` or `18` (Doctrine's `serverVersion`) |
| `{{venv.python}}` | `.venv\Scripts\python.exe` on Windows, `.venv/bin/python` elsewhere |
| `{{mvnw}}` | Maven's wrapper: `mvnw.cmd` on Windows, `sh mvnw` elsewhere (zips drop the executable bit) |
| `{{java_major}}` | The active Java's major version (`21`) |
| `{{db.scheme}}`, `{{db.charset}}` | `mysql` and `utf8mb4`, or `postgresql` and `utf8` (database URLs) |
| `{{db.pdo}}` | PDO's driver name: `mysql` (MariaDB too), `pgsql` or `sqlite`; empty when the project has no database, so `if: db.pdo == pgsql` adds PostgreSQL-only steps |
| `{{mail.host}}`, `{{mail.port}}` | Mailpit's SMTP server |
| `{{vars.<id>}}` | A variable's value |
| `{{random:N}}` | N random characters (1–256), safe inside quoted PHP and `.env` strings |

With `requires: node: true`:

| Placeholder | npm | pnpm | Yarn | Bun |
|---|---|---|---|---|
| `{{pm}}` | `npm` | `pnpm` | `yarn` | `bun` |
| `{{pm.run}}` | `npm run` | `pnpm run` | `yarn run` | `bun run` |
| `{{pm.args}}` | `--` | (nothing) | (nothing) | (nothing) |
| `{{pm.exec}}` | `npx -y` | `pnpm dlx` | `npx -y` | `bunx` |
| `{{pm.create}}` | `npm create` | `pnpm create` | `yarn create` | `bun create` |
| `{{pm.install}}`, `{{pm.add}}` | `npm install` | `pnpm install`, `pnpm add` | `yarn install`, `yarn add` | `bun install`, `bun add` |
| `{{node_version}}` | The Node.js version chosen in the wizard |

pnpm and Yarn run through `npx` when they aren't installed for that Node version; Bun has to be installed. `{{pm.args}}` separates a script's own arguments: `{{pm.run}} dev {{pm.args}} --port {port}`.

YAML reads a value that starts with `{` as a map, so quote values that begin with a placeholder: `"{{project}}"`.

### Serving: dynamic or static

```yaml
serve:
  mode: dynamic                 # dynamic (default) or static
  command: "{{pm.run}} dev {{pm.args}} --host 127.0.0.1 --port {port}"
  # file: server.js             # instead of a command: node runs this file
  # watch: true                 # with file: restart when files change
```

- **dynamic**: the domain forwards to a dev server the project runs. `{port}` (single braces) is the port LampRabit picks when the app starts; `$PORT` and `$HOST=127.0.0.1` are set too, for servers that read them. Dev servers that ignore them (Vite, Astro, Nuxt) need `--host 127.0.0.1 --port {port}`: `localhost` can mean IPv6 to them, where the proxy doesn't look.
- **static**: the web server serves the files: PHP for `.php`, and for JavaScript projects their build folder (`dist`, `build`, `out` or `public`, whichever has an `index.html`).

Dynamic needs `requires: node: true`. Projects without a template (or made before templates had `serve`) get the same treatment from their files: a package.json with a `dev`, `start:dev`, `start` or `serve` script, or a `server.js`-style file, is served dynamically; PHP projects are always served by PHP, and their package.json dev script (Vite for assets) runs next to the site without taking over the domain.

The domain follows what's listening. Every few seconds LampRabit looks for ports that the project's processes listen on: a process belongs to the project when it runs in the project folder, whether LampRabit started it or you did in your own terminal. Then:

- the domain goes to that port (with HTTPS when the site has it), and the web server restarts with the new route;
- with several ports (a dev server plus Storybook, or an HMR port), the lowest wins; pick another on the Sites page and LampRabit remembers it;
- when the port goes away, the domain switches back to the files after a few seconds (a dev server restarting itself doesn't flip it);
- PHP projects stay on PHP: their package.json dev server (Vite for assets) isn't routed unless you pick its port ("Route here", e.g. for `php artisan serve`), and "Back to PHP" undoes that.

**Start** on the Sites page keeps the project's port if it's still free, or picks the next free one from 3000, and starts the site's web server if it's stopped. A project's dev server only starts from the Sites page, never with Start all or a stack profile; a profile can start project processes it lists explicitly.

### Commands

```yaml
commands:
  discover: [composer.json, artisan]   # where to look; leave it out to detect from the files
  list:
    - name: Run migrations
      run: php artisan migrate
      description: Applies new database migrations.
```

The Commands tab lists the commands you saved, then the template's `list`, then what `discover` finds. Sources: `package.json` and `composer.json` scripts, `artisan`, `console` (Symfony's `bin/console`), `magento`, `please` (Statamic), `drush`, `cake` and `spark`. Commands run in the project folder with its PHP and Node.js versions first on PATH, one at a time per project, and their output goes to the project's log (Logs → the project). `run` takes the placeholders above.

### Processes

Long-running commands the project gets, listed under Sites → Commands → Keep running:

```yaml
processes:
  - name: queue                      # lowercase letters, digits, dashes; once each
    command: php artisan queue:work --tries=3
    restart: always                  # on-failure (default), always or never
    autostart: false                 # true (default): starts with the project
  - name: api
    cwd: api                         # a folder of the project; the project folder when absent
    command: node server.js --port {port}
    port: auto                       # a free port, or a number from 1024 up; also $PORT
    env: { NODE_ENV: development }
    if: vars.api                     # only when this holds, like a step's if:
```

`command`, `cwd` and `env` values take the placeholders above; `{port}` (single braces) is the process's port, filled in each time it starts. `port: auto` picks a different free port for every project made from the template.

### Domains

Subdomains of the project's own domain (Sites → Domains):

```yaml
domains:
  - name: admin                      # admin.<project>.test
    root: admin/public               # a folder of the project ("" for the main domain's)
  - name: api
    process: api                     # one of the processes above; it needs a port
  - name: docs
    port: 8090                       # anything listening on that port
  - name: preview                    # none of the three: wherever the main domain goes
    if: database
```

Give at most one of `root`, `port` and `process`. A subdomain pointing at a process that its `if:` left out is left out too.

### Making templates in the app

New project → **New template** opens a guided form (where the code comes from, PHP and the database, the usual install steps, processes and subdomains) with the YAML next to it, for anything the form doesn't cover. A project's Settings → **Save as a template** opens the editor on a template that recreates that project: a `git clone` of its remote, its Composer and Node installs, `.env` from `.env.example`, Laravel's key and migrations, its database, PHP version, dev server, saved commands, processes and subdomains. Either way the template is checked like one loaded from the folder before it's saved to `<root>/templates/<id>.yaml`.

## What happens when you click Create

1. The project name, database name and template are checked, and the site's PHP version, web server and HTTPS are saved.
2. If the template creates a database, the database server is started.
3. Requirements: PHP version, extensions, and the tools of the steps that will run. Nothing has been created yet, so a missing requirement leaves nothing behind.
4. The steps run in order, with their output shown live.
5. The web server restarts with the new site.

If a step fails, the project folder is removed and its settings are forgotten. A database created earlier in the run stays; delete it in phpMyAdmin or Adminer if you don't need it, or pick another database name when you try again.

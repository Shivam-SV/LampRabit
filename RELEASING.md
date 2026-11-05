# Releasing LampRabit

Releases are built by GitHub Actions ([.github/workflows/release.yml](.github/workflows/release.yml)) when a version tag is pushed. They land as a **draft** release with installers for every platform and the `latest.json` file the in-app updater reads.

## One-time setup

The updater only installs updates signed with LampRabit's key. The public half is in `src-tauri/tauri.conf.json` (`plugins.updater.pubkey`); the private half must never be committed.

1. The key pair was generated on the maintainer's machine at `~/.tauri/lamprabit-updater.key` (private) and `.key.pub` (public), with no password. **Back up the private key somewhere safe** (a password manager). If it's lost, existing installs can't be updated automatically any more.
2. In GitHub → repository → Settings → Secrets and variables → Actions, add:
   - `TAURI_SIGNING_PRIVATE_KEY`: the full contents of `lamprabit-updater.key`.
   - No password secret is needed: the key has none, and the workflow passes an empty `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. If you regenerate the key with a password, add it as a secret and reference it in release.yml.

To rotate the key: `npx tauri signer generate -w ~/.tauri/lamprabit-updater.key`, put the new `.pub` contents in `tauri.conf.json`, update the secret. Installs made with the old key won't accept updates signed with the new one.

### The old repository name

The repository was renamed from `LambRabit` to `LampRabit` after 0.1.1 shipped. Versions 0.1.0 and 0.1.1 check `github.com/Shivam-SV/LambRabit/...` for updates and rely on GitHub's rename redirect to reach this repository. **Never create a new repository called `LambRabit`** under this account: it would replace the redirect and those installs would stop receiving updates. Newer versions check the new name first and keep the old one only as a fallback.

### Code signing (recommended before a public launch)

Unsigned installers work but trigger warnings: Windows SmartScreen ("unrecognised app") and macOS Gatekeeper (macOS refuses unnotarised apps by default). When certificates are available:

- **Windows:** an OV/EV code-signing certificate; configure `bundle.windows.certificateThumbprint` or Tauri's `signCommand`, and the matching secrets.
- **macOS:** an Apple Developer ID certificate plus notarisation; tauri-action reads `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID`.

See https://v2.tauri.app/distribute/sign/ for details.

## Each release

1. Bump the version in **both** `package.json` and `src-tauri/tauri.conf.json` (and `src-tauri/Cargo.toml`) to the same value, for example `0.2.0`.
2. Commit, then tag and push:

   ```bash
   git tag v0.2.0
   git push origin main v0.2.0
   ```

3. Wait for the Release workflow (about 15–20 minutes for all platforms).
4. Open the draft release on GitHub, check the installers, write release notes, then **Publish**. From that moment the in-app updater offers it to existing installs.

## Building locally

```bash
# Windows example; installers end up in src-tauri/target/release/bundle/
export TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/lamprabit-updater.key)"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""
npx tauri build
```

Without the signing key the build still succeeds but warns that updater artifacts can't be signed.

## Smoke test

`.github/workflows/smoke.yml` installs the runtimes with the real app and starts, probes, restarts and stops every service on Windows, Linux and macOS. It runs on pull requests that touch the backend and weekly (with Elasticsearch, OpenSearch, Neo4j, RabbitMQ, pgAdmin, Go and Rust too); run it by hand from the Actions tab, optionally with a list of runtime ids. Locally, with a debug build:

```bash
cd src-tauri && cargo build
# Any empty folder; the app's own settings and folder are left alone. Linux: under xvfb-run.
./target/debug/lamprabit --smoke-test /tmp/lr-smoke redis php apache
```

It prints one line per step, writes `smoke-report.json` into the folder (its `logs/` has each service's output) and exits with 1 when anything failed.

## Own runtime builds (Apache, Nginx, PHP, Redis, Memcached)

Nobody publishes portable Linux or macOS builds of Apache, Nginx or PHP, so LampRabit builds its own, for Linux and macOS (x86_64 and aarch64 each). [.github/workflows/binaries.yml](.github/workflows/binaries.yml) runs the scripts in [build/linux/](build/linux/) and publishes the archives plus `manifest.json` (with a sha256 for each) to a pre-release tagged **`binaries`**. On Linux and macOS, the Services page reads that manifest (and on Windows, for Memcached). It's a pre-release on purpose: GitHub never treats it as "latest", so the app updater keeps finding app releases.

- **macOS** builds run natively on GitHub's macOS runners (`macos-15-intel`, `macos-latest`). Apache's APR libraries get `@rpath` install names and the edited binaries are re-signed ad hoc, which Apple Silicon requires.
- **Redis** is built core only (no modules, no TLS). Windows uses the community redis-windows build instead (no CI involved). Redis 8 is AGPLv3 among its licence options, so the release notes link the source; 7.2 is BSD.
- **Memcached** is built from source with libevent compiled in. For Windows, `build-memcached.sh <version> windows` repackages nono303's Cygwin build (GitHub's archive of the tag, checked against `MEMCACHED_WINDOWS_SHA256_x_y_z`) with Memcached's licence and a NOTICE for the Cygwin DLLs. A new Memcached version needs both checksums.
- **What gets built** is set in [build/linux/versions.env](build/linux/versions.env): Nginx and Apache versions, PHP release lines, and pinned versions and checksums of the libraries compiled in (OpenSSL, PCRE2, zlib, expat, APR). Apache and Nginx are built in Debian 11 (glibc 2.31), so they run on Ubuntu 20.04+, Debian 11+, Fedora 32+ and newer. PHP is built fully static with [static-php-cli](https://github.com/crazywhalecc/static-php-cli), with the extensions listed in `PHP_EXTENSIONS` compiled in (static PHP can't load extensions later, so `redis`, `memcached` and `igbinary` are among them, and the build fails if one is missing).
- **Licences:** every archive carries the runtime's own licence and a `licenses/` folder with those of the libraries compiled in (`licenses` in lib.sh; for PHP, `spc dump-license`). PHP's static builds contain LGPL code (libiconv, and gettext on macOS), and Redis 8 is offered under AGPLv3: the sources are the pinned upstream tarballs plus `build/linux/`, both public. macOS builds target macOS 11 (`MACOSX_DEPLOYMENT_TARGET`), like the app.
- **When it runs:** on pushes to `main` that change `build/linux/`, every Monday (to pick up new PHP patch releases), or by hand from the Actions tab. Builds already in the manifest are skipped; tick "rebuild" to replace them.
- **New Apache or Nginx version:** edit `versions.env` (for Apache, add its `HTTPD_SHA256_x_y_z` from downloads.apache.org) and push. Older builds stay in the manifest, so existing installs keep working.
- **Testing builds locally:** run a script on any Linux machine (or WSL), for example `bash build/linux/build-nginx.sh 1.30.6`. Archives land in `out/`. To point the app at them, serve the folder (`python3 build/linux/manifest.py out > out/manifest.json && python3 -m http.server -d out 8000`) and start LampRabit with `LAMPRABIT_BINARIES_URL=http://localhost:8000/`.

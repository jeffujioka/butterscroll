# mfx: release, install, update and Start at login

Status: design, approved decisions; not implemented. Terms follow [GLOSSARY.md](../../../GLOSSARY.md). Related decisions: [ADR 0001](../../adr/0001-self-signed-release-cert.md) (signing), [ADR 0002](../../adr/0002-main-app-login-item.md) (login item).

## Intent

mfx is today a bare binary you build and run from `target/release/`. The maintainer wants to publish it so anyone can install it, ship new Versions that existing users can update to, and have mfx start at login with a switch in the Settings window. Everything must work for free: the maintainer will not pay for the Apple Developer Program.

## Goals

- Anyone on macOS 13+ (Apple Silicon or Intel) installs mfx with one command and gets a working menu-bar app.
- A user updates to a newer Release from inside mfx, keeping their Accessibility grant, their Start at login state and their config.
- Start at login is on after First launch, and the Settings toggle and System Settings → Login Items always agree.
- The maintainer publishes a Release by tagging; the build is reproducible in CI (covered by the release pipeline, see "Release pipeline contract").

## Non-goals

- Notarization, Developer ID, Mac App Store, Homebrew (official cask is closed to apps that fail Gatekeeper since 2026-09-01).
- Silent background installs of updates.
- Downgrades, multi-user installs that share one bundle across accounts, migration from the old hand-written LaunchAgent (only the maintainer ever had it).
- Restarting mfx after a crash (see ADR 0002).

## Anti-goals (would be a failure even if it technically works)

- An update that silently loses the user's Accessibility grant, overwrites their config, or leaves them with no working app.
- Two copies of mfx running at once (every scroll and remapped button would fire twice).
- mfx switching Start at login back on after the user turned it off anywhere.
- Installing anything not signed with the release certificate.

## Constraints

- No paid Apple program: a browser-downloaded app hits Gatekeeper ("Open Anyway"); files fetched by `curl` or by mfx itself are not quarantined and do not.
- macOS ties Accessibility to the code signature and grants it to the *responsible* process: Terminal when mfx is started from a shell, mfx itself when LaunchServices starts the bundle (`src/main.rs:160`).
- `/Applications` is `root:admin 0775`: standard (non-admin) accounts can't write it.
- `SMAppService` (macOS 13+): `mainApp` launches only at login, no crash relaunch; `unregister()` of an agent kills it; `status` returns `.requiresApproval` when the user revoked consent in System Settings (SDK `SMAppService.h`).
- winit 0.30.13 exposes no "reopen" (Finder double-click on a running app) callback.
- Config structs use `#[serde(deny_unknown_fields)]` and `#[serde(default)]` (`src/config.rs:103-169`); the Settings window is a separate process spawned via `current_exe()` (`src/macos/app.rs:153`) and today overwrites an unparseable config on the first edit (`src/settings.rs:40-42`).

## Approaches considered

| Area | Options | Picked | Why |
|---|---|---|---|
| Distribution | Homebrew cask; DMG/zip download only; `curl … \| bash` install script + zip | Install script + zip fallback | Homebrew requires notarization; browser downloads hit Gatekeeper; curl downloads don't |
| Signing | Developer ID ($99/yr); ad-hoc; Apple Development cert; self-signed cert | One self-signed cert, 20-year validity (ADR 0001) | Free, and a stable signature keeps the Accessibility grant across updates and gives the updater an identity to check |
| Updates | Sparkle; notify-only; custom in-app updater | Custom updater, user-initiated | Sparkle is an Obj-C framework that is awkward in a winit/Rust app; notify-only leaves users to re-run the script |
| Start at login | Hand-written LaunchAgent (today); bundled agent via `SMAppService.agent`; `SMAppService.mainApp` | `mainApp` (ADR 0002) | Only `mainApp` lets the toggle read real state without quitting mfx when turned off |
| Install location | `~/.local/bin` binary; `/Applications/mfx.app`; `~/Applications/mfx.app` | `mfx.app` in `/Applications`, `~/Applications` when not writable, plus `~/.local/bin/mfx` symlink | GUI app with Accessibility and Login Items needs a bundle; the symlink keeps the CLI |

## Design

### Packaging and identity

- Ship `mfx.app`: `CFBundleIdentifier=com.jeffujioka.mfx`, `CFBundleExecutable=mfx`, `LSUIElement=1`, `LSMinimumSystemVersion=13.0`, universal (arm64 + x86_64). Built by the release pipeline.
- Every Release is signed with the release certificate. Its designated requirement (DR) is `identifier "com.jeffujioka.mfx" and certificate leaf = H"<sha1>"`, committed as `packaging/designated-requirement.txt` and compiled into mfx as a constant.
- Versions come from `Cargo.toml`. `mfx --version` prints exactly `mfx X.Y.Z[-pre]`; the menu's first line is a greyed `mfx X.Y.Z`. Comparisons with a tag strip its leading `v`.

### Process model

- **Canonical path.** Any mfx process whose `current_exe()` differs from its canonicalized path re-execs itself via the canonical path first, so `NSBundle.mainBundle` and `SMAppService.mainApp` resolve to `mfx.app` even when started through `~/.local/bin/mfx`.
- **Single instance.** The menu-bar app holds an `flock` on `~/Library/Application Support/mfx/mfx.lock` (released by the OS on crash). A second `mfx run` that can't take it spawns `mfx settings` and exits 0. The Settings process holds `settings.lock`; a second one exits immediately. Finder/Spotlight "reopen" of a running mfx opens Settings by adding `applicationShouldHandleReopen:hasVisibleWindows:` to winit's application-delegate class; if the spike shows that can't be done safely, reopen does nothing. CLI subcommands other than `run` and `settings` take no lock.
- **Starting mfx.** `install.sh` and the updater always start it with `open "<install dir>/mfx.app"`, never the binary, so mfx (not Terminal) is the responsible process. `mfx` / `mfx run` from a terminal stays as the developer path and keeps today's `permission_hint`.
- **Logging.** When stderr is not a TTY, `run` redirects it (append) to `~/Library/Logs/mfx.log`; if that file exceeds 1 MB at startup it is first renamed to `mfx.old.log`. Existing `eprintln!` calls stay.

### Start at login

- macOS is the only store of this state; nothing goes in `config.toml`.
- **First launch:** only when mfx runs from inside a bundle whose identifier is `com.jeffujioka.mfx` (a bare `target/release/mfx` or `cargo run` never registers and never writes the marker; builder's call). mfx triggers the Accessibility prompt (as today), then calls `SMAppService.mainApp.register()` once, whether or not Accessibility was granted, and creates the empty marker `~/Library/Application Support/mfx/login-registered`. When the marker exists, mfx never registers by itself again. macOS shows its own "Background Items Added" notice. The single-instance lock applies to the developer path too (builder's call: two copies firing every event twice is never wanted).
- **Settings toggle** (in the Settings window): reads `status` when the window opens and whenever it regains focus. On → `register()`, off → `unregister()`. `.requiresApproval` shows as off plus an "Open Login Items…" button calling `SMAppService.openSystemSettingsLoginItems()`. Changes made in System Settings show up on the next focus.
- **CLI:** `mfx login-item on|off|status` (used by `install.sh --uninstall`).
- Remove `mfx launch-agent`, its README section (including the "launchd restarts mfx if it crashes" line) and the `permission_hint` text pointing to it (`src/main.rs:168`).
- Known limit, pending the spike: an entry added by hand with "+" under Open at Login may not be visible to `status` (`statusForLegacyURL` only covers launchd plists). If so, README says to use mfx's toggle.

### Updates

- **Setting:** `[updates] check_automatically = false` by default, editable in the Settings window. When on, mfx checks once a day. "Check for Updates…" is always in the menu.
- **Check:** `GET https://api.github.com/repos/<owner>/<repo>/releases/latest` (excludes Drafts and Pre-releases). The owner/repo lives in one constant (the name may change). `MFX_UPDATE_URL` overrides the URL for testing. A newer Version than the running one adds a menu item "Update to vX.Y.Z…". Nothing installs without that click.
- **Apply:**
  1. Download `mfx-vX.Y.Z-universal.zip`.
  2. Unpack with `ditto -x -k` into `<install dir>/.mfx-update.app` (same volume as the running bundle).
  3. Verify: `codesign --verify --deep --strict -R=<DR>` and `CFBundleShortVersionString` == tag without `v`.
  4. Quit the Settings child process.
  5. Swap with `renamex_np(RENAME_SWAP)`, then remove the old bundle (now at the staging name).
  6. Spawn a detached `/bin/sh -c 'while kill -0 <pid>; do sleep 0.2; done; open "<app>"'` and exit.
- **Failures:** any failure before the swap removes the staging copy, leaves the running app untouched and offers the Release page. If the install dir isn't writable or the bundle isn't owned by the current user, mfx goes straight to the Release page.
- The `<install dir>` is wherever the running bundle lives (from `current_exe()`), not a hard-coded path.

### Config compatibility

- A new Release never rejects a config an older one accepted: renamed keys keep the old name via `#[serde(alias)]`; removed keys stay as `#[serde(default, skip_serializing)]` placeholder fields. `deny_unknown_fields` stays (typo detection).
- Settings opens **read-only** when the file doesn't parse: it shows the error and a "Reveal in Finder" button and writes nothing until the file parses. Replaces "Showing defaults; editing will overwrite it".
- Running an older Version against a newer config is unsupported.

### install.sh

Hosted at `raw.githubusercontent.com/<owner>/<repo>/HEAD/install.sh` (`HEAD` = the default branch, whatever it is named); run with `curl -fsSL … | bash`. The owner/repo is still open (the name may change) and must be set in its one constant before the first Release; the README command and the updater use the same value.

- **Install, update, repair:** same script.
  1. Pick the install dir: if `mfx.app` already exists in `/Applications` or `~/Applications`, reuse that location; if that location isn't writable, stop and explain (never create a second copy). Otherwise `/Applications` if writable, else `~/Applications` (created). (Builder's call.)
  2. Fetch the latest Release zip; `--version vX.Y.Z` fetches that tag instead (this is how testers opt in to a Pre-release and how the maintainer installs a specific Release); or use `MFX_INSTALL_ZIP=<path>`. A local zip has its quarantine attribute stripped before unpacking, since it still goes through the signature check. (Builder's call.)
  3. Unpack to a staging dir and verify it exactly like the updater.
  4. Quit mfx and the Settings process.
  5. Replace the app.
  6. Create `~/.local/bin` if missing; (re)create the `mfx` symlink; warn if `~/.local/bin` isn't on `PATH`.
  7. `open` the app.
- **`--uninstall [--purge]`:**
  1. Quit mfx and the Settings process.
  2. Run `<install dir>/mfx.app/Contents/MacOS/mfx login-item off` (real path).
  3. `tccutil reset Accessibility com.jeffujioka.mfx` (failure ignored, manual hint printed).
  4. Remove the app, the symlink, `~/Library/Application Support/mfx/`, `~/Library/Logs/mfx.log` and `mfx.old.log`.
  5. Remove `~/.config/mfx/` only with `--purge`.
  6. If the app is already gone, print "remove mfx under System Settings → General → Login Items".
- README documents the zip fallback: "Open Anyway" in Privacy & Security, or `xattr -dr com.apple.quarantine <install dir>/mfx.app`.

### Release pipeline contract

The pipeline (GitHub Actions on `v*` tags, `scripts/release.sh`, `packaging/`, `RELEASING.md`, `CHANGELOG.md`) is a separate workstream. This design relies on it for:

- Asset `mfx-v<version>-universal.zip` built with `ditto -c -k --keepParent` (exactly one `mfx.app/` at the root) plus `<zip>.sha256`.
- `CFBundleShortVersionString` = full SemVer from `Cargo.toml` (`0.2.0-rc.1` for Pre-releases), `CFBundleVersion` numeric.
- Signing with the release cert; CI verifies the unpacked app with the same DR and version check as `install.sh` and the updater.
- CI creates a Draft; `release.sh promote vX.Y.Z` publishes it and keeps GitHub's "latest" on the highest final Version.

Sequencing (builder's call): the pipeline's files are already written in the main checkout, uncommitted. They land on this branch before the updater phase. Until the maintainer creates the release certificate, development and the spike use a local test self-signed certificate; the DR constant is read at build time from `packaging/designated-requirement.txt`.

## Verify before building: spike

Throwaway, outside the repo, on a hand-assembled self-signed `mfx.app`:

1. Accessibility grant survives an update signed with the same cert, on a second macOS user account.
2. `SMAppService.mainApp` register/status works for a self-signed bundle and from the Settings child process.
3. Whether `status` sees an entry added with "+" in Login Items.
4. The reopen hook on winit's delegate class fires.
5. Replacing the bundle in `/Applications` from mfx itself and from `install.sh` in Terminal isn't blocked by macOS App Management protection (the bundle is not notarized and has no Team ID).
6. Start at login still works after the bundle is swapped for one with a new code hash.
7. `NSBundle.mainBundle` when started through the symlink (does the canonical re-exec matter).

A failure in 1, 2, 5 or 6 changes this design and comes back to the maintainer before any implementation. Failures in 3, 4 or 7 have the fallbacks written above (known limit, reopen does nothing, re-exec stays).

## Left to the builder

- How HTTP and JSON are done (shelling out to `/usr/bin/curl` vs a crate), objc2 crate versions, and how the Obj-C calls are wrapped.
- Settings window layout for the Start at login toggle and the update setting.
- The internal mechanism that tells a running main instance about Settings (beyond the lock rules above).
- `install.sh` structure and messages, as long as the steps and checks above hold.
- Test strategy: behavior in pure Rust (version compare, path logic, config compatibility) gets unit tests; macOS-integration paths get the spike and manual smoke runs (local fake release via `MFX_UPDATE_URL`, `MFX_INSTALL_ZIP`).

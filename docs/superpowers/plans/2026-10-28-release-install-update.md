# mfx release, install, update and Start at login: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn mfx from a bare binary into a signed `mfx.app` that installs with one command, updates itself on request, and turns Start at login on at First launch with a Settings toggle that mirrors macOS.

**Architecture:** Pure, unit-tested Rust modules hold the decisions (versions, paths, release parsing, first-launch rule, config compatibility); thin macOS modules wrap the system (SMAppService via objc2, flock, `renamex_np`, `codesign`/`ditto`/`curl`/`plutil` subprocesses). `install.sh` mirrors the updater's verification in shell. The release pipeline (`scripts/release.sh`, `packaging/`) already exists in the main checkout and is imported, not rewritten.

**Tech Stack:** Rust 2021, winit 0.30, tray-icon 0.26, eframe/egui 0.35, objc2 0.6 + objc2-foundation 0.3 + objc2-service-management 0.3, libc 0.2, serde_json 1, bash.

**Spec:** `docs/superpowers/specs/2026-10-28-release-install-update-design.md`

## Global Constraints

- Minimum macOS 13.0; universal arm64 + x86_64 (built by `scripts/release.sh`).
- Bundle identifier `com.jeffujioka.mfx`; app `mfx.app`, executable `mfx`.
- `mfx --version` prints exactly `mfx <CARGO_PKG_VERSION>`; tags compare with their leading `v` stripped.
- Release asset name: `mfx-v<version>-universal.zip` (+ `.sha256`), exactly one `mfx.app/` at the zip root.
- Designated requirement form: `identifier "com.jeffujioka.mfx" and certificate leaf = H"<lowercase sha1>"`, source of truth `packaging/designated-requirement.txt`.
- App data: `~/Library/Application Support/mfx/` (`mfx.lock`, `settings.lock`, `login-registered`); logs `~/Library/Logs/mfx.log`, rollover to `mfx.old.log` above 1 048 576 bytes.
- `[updates] check_automatically` defaults to `false`; check interval 24 h; lookup `GET https://api.github.com/repos/<REPO>/releases/latest`, `MFX_UPDATE_URL` overrides.
- No TLS/HTTP crate: network goes through `/usr/bin/curl`. No Sparkle, no notarization.
- Errors stay `Result<_, String>` with `eprintln!("mfx: …")` logging, as in the existing code. Tests are inline `#[cfg(test)] mod tests` like `src/config.rs`.
- macOS-only calls from cross-platform modules (`main.rs`, `settings.rs`) sit behind `#[cfg(target_os = "macos")]`, like the existing `run`/`check_permissions`. Tests never mutate process environment variables (tests run in parallel); functions that read env take the value as a parameter.
- Never create a second installed copy; never overwrite an unparseable config; never register Start at login after the marker exists.

## Review Focus

1. Network down, GitHub rate limit (HTTP 403 JSON body) or malformed JSON during a check: no crash, no update offered, retried at the next interval. Pinned in Task 9 (`parse_latest_rejects_error_body`, `parse_latest_rejects_garbage`).
2. A published Release without the expected asset (half-uploaded): not offered. Pinned in Task 9 (`parse_latest_requires_matching_asset`).
3. Corrupt or truncated download / failed verification: staging removed, installed bundle byte-identical. Pinned in Task 10 (`apply_with_corrupt_zip_leaves_bundle_untouched`).
4. A config file written by 0.1.0 (no `[updates]`): still loads after updating. Pinned in Task 8 (`config_from_0_1_0_still_parses`).
5. `register()` fails at First launch (bad signature, denied): marker not written, retried next launch; a dev binary outside the bundle never registers. Pinned in Task 6 (`first_launch_rules`).

---

### Task 1: Import the release pipeline onto this branch

**Files:**
- Create (copied from the main checkout): `scripts/release.sh`, `packaging/Info.plist`, `.github/workflows/release.yml`, `rust-toolchain.toml`, `CHANGELOG.md`, `RELEASING.md`
- Modify: `.gitignore` (add the `dist/` line from the main checkout's version)

- [ ] **Step 1:** Confirm with the maintainer that the Q12 session (herdr agent `handoff-mfx-release`) has finished editing those files.
- [ ] **Step 2:** Copy them into the worktree with `cp -p`, preserving the executable bit on `scripts/release.sh`.
- [ ] **Step 3:** Run `scripts/release.sh --dry-run build && scripts/release.sh --dry-run package`. Expected: `dist/mfx.app` exists, `lipo -archs dist/mfx.app/Contents/MacOS/mfx` prints `x86_64 arm64`, warnings only about ad-hoc signing and the missing DR file.
- [ ] **Step 4:** Commit: `git add -A scripts packaging .github rust-toolchain.toml CHANGELOG.md RELEASING.md .gitignore && git commit -m "build: import release pipeline"`.

### Task 2: Spike (throwaway, gate for everything after)

**Files:** none in the repo. Work in `~/.local/tmp/mfx-spike/`. Record results in the spec under a new `## Spike results` section.

- [ ] **Step 1:** Create a test self-signed code-signing certificate `mfx spike` (Keychain Access → Certificate Assistant, type Code Signing). Get its SHA-1 with `security find-certificate -c "mfx spike" -Z | awk '/SHA-1/{print $3}'`.
- [ ] **Step 2:** Build a signed bundle: `MFX_SIGN_ID=<sha1> scripts/release.sh --dry-run build package`. Copy `dist/mfx.app` to `/Applications`.
- [ ] **Step 3 (spike item 2):** A throwaway Swift script, `swift spike-login.swift register|status`, calls `SMAppService.mainApp` while running *inside* the bundle. Copy the compiled helper into `Contents/MacOS/` and re-sign. Run `register`, then `status`. Expected: `enabled`, and an mfx entry under Login Items. Repeat from a child process spawned by the helper.
- [ ] **Step 4 (item 3):** Remove the entry, add `/Applications/mfx.app` with "+" under Open at Login, run `status`. Record what it reports.
- [ ] **Step 5 (items 1, 5, 6):** Needs the maintainer. As the second macOS user:
  1. Open the bundle and grant Accessibility.
  2. Rebuild with a bumped `Cargo.toml` version, sign with the same cert.
  3. Swap the bundle with `mv` from Terminal (item 5, Terminal path) and from the bundle's own helper (item 5, self path).
  4. Relaunch.

  Expected: no App Management block, the Accessibility toggle is still on, and Login Items still lists mfx and starts it at the next login (item 6).
- [ ] **Step 6 (item 4):** Add `applicationShouldHandleReopen:hasVisibleWindows:` to `NSApp.delegate`'s class with `class_addMethod` in a scratch copy of `src/macos/app.rs`, logging when it is called. Run `open /Applications/mfx.app` twice. Expected: the log line on the second `open`.
- [ ] **Step 7 (item 7):** `ln -s /Applications/mfx.app/Contents/MacOS/mfx /tmp/mfx-link`, then run the Step 3 helper through a symlink. Record `NSBundle.mainBundle.bundleIdentifier`.
- [ ] **Step 8:** Write the results to the spec. If item 1, 2, 5 or 6 failed, **stop and bring the result to the maintainer**. Otherwise commit: `git commit -am "docs: spike results"`.

### Task 3: Identity, versions and `--version`

**Files:**
- Create: `src/version.rs`, `src/paths.rs`
- Modify: `src/main.rs:16-29` (USAGE, replace `LABEL`), `src/macos/app.rs:28-36,197-218` (version menu line)

**Interfaces:**
- Produces: `paths::BUNDLE_ID: &str = "com.jeffujioka.mfx"`.
- Produces: `version::CURRENT: &str = env!("CARGO_PKG_VERSION")` and `version::Version { major: u64, minor: u64, patch: u64, pre: Option<String> }`, which implements `Ord`, `Display` and `FromStr`.
- Produces: `Version::parse(&str) -> Option<Version>`, `Version::from_tag(&str) -> Option<Version>` (strips one leading `v`), `Version::current() -> Version` and `Version::is_prerelease(&self) -> bool`.

- [ ] **Step 1: Write the failing tests** in `src/version.rs`:

```rust
#[test] fn parses_final_and_prerelease() {
    assert_eq!(Version::parse("0.2.0").unwrap().to_string(), "0.2.0");
    assert_eq!(Version::parse("0.2.0-rc.1").unwrap().pre.as_deref(), Some("rc.1"));
    assert!(Version::parse("0.2").is_none() && Version::parse("v0.2.0").is_none());
}
#[test] fn from_tag_strips_one_v() {
    assert_eq!(Version::from_tag("v1.2.3"), Version::parse("1.2.3"));
    assert!(Version::from_tag("vv1.2.3").is_none());
}
#[test] fn semver_precedence() {
    let v = |s| Version::parse(s).unwrap();
    assert!(v("0.2.0-rc.1") < v("0.2.0"));
    assert!(v("0.2.0-rc.2") < v("0.2.0-rc.10"));   // numeric identifiers compare as numbers
    assert!(v("0.10.0") > v("0.9.9"));
    assert!(v("1.0.0-alpha") < v("1.0.0-alpha.1"));
}
#[test] fn current_is_valid() { assert_eq!(Version::current().to_string(), CURRENT); }
```

- [ ] **Step 2:** `cargo test version::` → FAIL (module missing).
- [ ] **Step 3:** Implement `Version` (SemVer 2.0 precedence: pre-release lower than final; dot identifiers, numeric ones compared numerically, numeric < alphanumeric, shorter prefix lower). Add `mod version; mod paths;` to `main.rs`. Move `LABEL` to `paths::BUNDLE_ID`.
- [ ] **Step 4:** Add the `--version` / `-V` flag in `parse_args` → command `version` → `println!("mfx {}", version::CURRENT)`. Add it to USAGE.
- [ ] **Step 5:** In `app.rs`, add `Items.version: MenuItem::new(format!("mfx {}", version::CURRENT), false, None)` as the first menu entry, before `status`.
- [ ] **Step 6:** `cargo test && cargo run -- --version` → tests pass; prints `mfx 0.1.0`.
- [ ] **Step 7:** Commit: `feat: version parsing, --version and menu version line`.

### Task 4: Canonical re-exec and log file

**Files:**
- Create: `src/macos/startup.rs`
- Modify: `src/paths.rs`, `src/macos/mod.rs`, `src/main.rs:108-123,177-183`

**Interfaces:**
- Produces: `paths::app_support_dir() -> Option<PathBuf>`, `paths::log_file() -> Option<PathBuf>`, `paths::old_log_file() -> Option<PathBuf>` (all under `$HOME`).
- Produces: `paths::bundle_of(exe: &Path) -> Option<PathBuf>`, which returns `X.app` for `…/X.app/Contents/MacOS/<name>`.
- Produces: `startup::reexec_if_symlinked() -> Result<(), String>`, which only returns when no re-exec happened.
- Produces: `startup::rotate_if_larger(log: &Path, old: &Path, max: u64) -> std::io::Result<bool>` and `startup::redirect_stderr(log: &Path) -> std::io::Result<()>`.
- Produces: `startup::LOG_MAX_BYTES: u64 = 1_048_576`.

- [ ] **Step 1: Write the failing tests:**

```rust
// paths.rs
#[test] fn bundle_of_finds_app_root() {
    assert_eq!(bundle_of(Path::new("/Applications/mfx.app/Contents/MacOS/mfx")), Some(PathBuf::from("/Applications/mfx.app")));
    assert_eq!(bundle_of(Path::new("/x/target/release/mfx")), None);
    assert_eq!(bundle_of(Path::new("/x/mfx.app/Contents/Resources/mfx")), None);
}
// startup.rs (temp dir under std::env::temp_dir(), unique per test)
#[test] fn rotates_only_above_limit() {
    // 10-byte log, max 5 → Ok(true); log gone; old has the 10 bytes (an existing old file is replaced)
    // 3-byte log, max 5 → Ok(false); log untouched
    // missing log → Ok(false)
}
```

- [ ] **Step 2:** `cargo test` → FAIL.
- [ ] **Step 3:** Implement. `reexec_if_symlinked`: if `current_exe()` != `canonicalize(current_exe())`, `exec` the canonical path with the same args (`std::os::unix::process::CommandExt::exec`). `redirect_stderr`: open append + `libc::dup2` onto fd 2. Add `libc = "0.2"` to `Cargo.toml`.
- [ ] **Step 4:** Wire it up:
  - Call `reexec_if_symlinked()` first in `main()`, on macOS.
  - In `run()`, when `!std::io::stderr().is_terminal()`: `rotate_if_larger(log, old, LOG_MAX_BYTES)`, then `redirect_stderr(log)`. A failure is printed and ignored.
  - `mfx check` prints `binary: <current_exe>` as its first line.
- [ ] **Step 5:** Verify:
  - `cargo test` passes.
  - `ln -sf "$PWD/target/debug/mfx" /tmp/mfx-link && /tmp/mfx-link check` prints `binary: …/target/debug/mfx`, not the link.
  - `target/debug/mfx run 2>/dev/null </dev/null &` writes `mfx: …` lines to `~/Library/Logs/mfx.log`. Kill it afterwards.
- [ ] **Step 6:** Commit: `feat: canonical re-exec and log file`.

### Task 5: Single instance

**Files:**
- Create: `src/macos/instance.rs`
- Modify: `src/paths.rs`, `src/main.rs:177-183`, `src/settings.rs:36-48`

**Interfaces:**
- Produces: `paths::main_lock() -> Option<PathBuf>` and `paths::settings_lock() -> Option<PathBuf>`.
- Produces: `instance::InstanceLock` (holds the `File`; the lock is released on drop or exit).
- Produces: `instance::try_acquire(path: &Path) -> std::io::Result<Option<InstanceLock>>`, which creates parent dirs and uses `flock(LOCK_EX | LOCK_NB)`.

- [ ] **Step 1: Write the failing test:**

```rust
#[test] fn second_acquire_fails_until_first_dropped() {
    let p = unique_temp_dir().join("x.lock");
    let a = try_acquire(&p).unwrap();
    assert!(a.is_some());
    assert!(try_acquire(&p).unwrap().is_none());
    drop(a);
    assert!(try_acquire(&p).unwrap().is_some());
}
```

- [ ] **Step 2:** `cargo test instance::` → FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Wire it up:
  - `run()`: take `main_lock()`. If it's held, spawn `current_exe() settings --config <path>`, print `mfx: already running; opening Settings`, and return `Ok(())`. Keep the lock alive for the whole run.
  - `settings::run`: take `settings_lock()`. If it's held, return `Ok(())`.
- [ ] **Step 5:** Verify: `cargo test`. Then start `target/debug/mfx` in two terminals. Expected: one menu-bar icon; the second terminal prints `already running` and a Settings window opens; running `mfx settings` again opens no second window.
- [ ] **Step 6:** Commit: `feat: single running instance`.

### Task 6: Start at login, First launch and `login-item` CLI

**Files:**
- Create: `src/macos/login_item.rs`, `src/first_launch.rs`
- Modify: `Cargo.toml` (`objc2 = "0.6"`, `objc2-foundation = { version = "0.3", features = ["NSBundle", "NSString", "NSError"] }`, `objc2-service-management = { version = "0.3", features = ["SMAppService"] }`), `src/paths.rs`, `src/main.rs` (USAGE, remove `launch_agent_plist` 73-106 and its match arm 135-138, rewrite `permission_hint` 160-175), `src/macos/app.rs:237-247`

**Interfaces:**
- Consumes: `paths::BUNDLE_ID` and `paths::app_support_dir()`.
- Produces: `paths::login_marker() -> Option<PathBuf>`.
- Produces: `login_item::Status { NotRegistered, Enabled, RequiresApproval, NotFound }` with `Status::as_cli_str(self) -> &'static str` returning `"disabled" | "enabled" | "requires-approval" | "not-found"`.
- Produces: `login_item::status() -> Status`.
- Produces: `login_item::set_enabled(on: bool) -> Result<(), String>`. `kSMErrorAlreadyRegistered` on `true` and `kSMErrorJobNotFound` on `false` count as `Ok`.
- Produces: `login_item::open_system_settings()` and `login_item::main_bundle_id() -> Option<String>`.
- Produces: `first_launch::should_register(marker_exists: bool, bundle_id: Option<&str>) -> bool` and `first_launch::write_marker(register: &Result<(), String>) -> bool`.

- [ ] **Step 1: Write the failing tests** in `src/first_launch.rs`:

```rust
#[test] fn first_launch_rules() {
    assert!(should_register(false, Some("com.jeffujioka.mfx")));
    assert!(!should_register(true, Some("com.jeffujioka.mfx")));   // never again
    assert!(!should_register(false, None));                         // bare binary / cargo run
    assert!(!should_register(false, Some("com.other")));
    assert!(write_marker(&Ok(())));
    assert!(!write_marker(&Err("kSMErrorInvalidSignature".into())));   // retry next launch
}
```

Plus in `login_item.rs`: `cli_strings` checks the four `as_cli_str` values.
- [ ] **Step 2:** `cargo test` → FAIL.
- [ ] **Step 3:** Implement the two modules. `login_item` wraps `SMAppService::mainAppService()`, `status()`, `registerAndReturnError` / `unregisterAndReturnError` and `openSystemSettingsLoginItems`; it gets `NSBundle::mainBundle().bundleIdentifier()` from objc2-foundation.
- [ ] **Step 4:** First launch in `app.rs` `new_events` (Init), after `tap::accessibility_trusted(true)`: if `should_register(marker.exists(), main_bundle_id().as_deref())`, call `set_enabled(true)`; if `write_marker(&result)`, create the empty marker file; log errors.
- [ ] **Step 5:** CLI `mfx login-item on|off|status` in `real_main`. `status` prints `as_cli_str()`; `on`/`off` call `set_enabled`. Remove `launch-agent` everywhere. `permission_hint`'s terminal branch ends with: `Either enable {term} under {PANE} and restart it, or start the app with "open -a mfx" so the mfx entry applies.`
- [ ] **Step 6:** Verify:
  - `cargo test` passes.
  - Build the signed bundle as in Task 2 Step 2 and copy it to `/Applications`. Run `rm -f ~/Library/Application\ Support/mfx/login-registered`, then `open /Applications/mfx.app`.
  - Expected: `/Applications/mfx.app/Contents/MacOS/mfx login-item status` prints `enabled`, the marker exists, and mfx is listed under Login Items.
  - Then `… login-item off` → `disabled`. Relaunch → stays `disabled`.
- [ ] **Step 7:** Commit: `feat: Start at login via SMAppService, First launch registration`.

### Task 7: Reopen opens Settings

Only if spike item 4 passed; otherwise skip this task and note it in the spec.

**Files:**
- Create: `src/macos/reopen.rs`
- Modify: `src/macos/app.rs:22-26` (`UserEvent::Reopen`), `src/macos/app.rs:49-80`, `src/macos/app.rs:254-269`

**Interfaces:**
- Produces: `reopen::install(proxy: winit::event_loop::EventLoopProxy<UserEvent>)`. Call it after the event loop has started; it adds `applicationShouldHandleReopen:hasVisibleWindows:` (returning `NO`) to `NSApp.delegate`'s class and sends `UserEvent::Reopen`.

- [ ] **Step 1:** Implement it, using the code proven in spike Step 6. Handle `UserEvent::Reopen` with `self.open_settings()`.
- [ ] **Step 2:** Verify: with the bundle running, `open /Applications/mfx.app` opens Settings and leaves only one mfx process (`pgrep -x mfx | wc -l` → `1` plus the settings process).
- [ ] **Step 3:** Commit: `feat: reopening mfx.app opens Settings`.

### Task 8: `[updates]` config section and compatibility fixture

**Files:**
- Create: `src/fixtures/config-0.1.0.toml` (byte copy of the current `src/default-config.toml`)
- Modify: `src/config.rs:14-20,102-112,255-326`, `src/default-config.toml`

**Interfaces:**
- Produces: `ConfigFile.updates: UpdatesFile` (`#[serde(default)]`), with `#[serde(deny_unknown_fields, default)] pub struct UpdatesFile { pub check_automatically: bool }` defaulting to `false`.
- Produces: `Config.check_updates: bool`.

- [ ] **Step 1: Write the failing tests:**

```rust
#[test] fn updates_default_off() { assert!(!Config::default().check_updates); assert!(!Config::parse("").unwrap().check_updates); }
#[test] fn updates_can_be_enabled() { assert!(Config::parse("[updates]\ncheck_automatically = true").unwrap().check_updates); }
#[test] fn config_from_0_1_0_still_parses() { Config::parse(include_str!("fixtures/config-0.1.0.toml")).unwrap(); }
```

- [ ] **Step 2:** `cargo test config::` → FAIL.
- [ ] **Step 3:** Implement. Append to `default-config.toml`:

```toml
[updates]
# Check GitHub once a day for a newer Release. Updates are only installed when you choose
# "Update to …" in the menu.
check_automatically = false
```

- [ ] **Step 4:** `cargo test` → PASS (including the existing `saved_file_reloads_to_the_same_config`).
- [ ] **Step 5:** Commit: `feat: updates.check_automatically setting (off by default)`.

### Task 9: Release lookup and update decision

**Files:**
- Create: `src/update.rs`, `src/fixtures/release-latest.json` (trimmed GitHub response with `tag_name`, `html_url`, `draft`, `prerelease` and two `assets`), `src/fixtures/rate-limited.json` (`{"message":"API rate limit exceeded …","documentation_url":"…"}`)
- Modify: `Cargo.toml` (`serde_json = "1"`)

**Interfaces:**
- Consumes: `version::Version`.
- Produces: `update::REPO: &str = "jeffujioka/mouse-fix-rs"` (the one place to rename), `update::latest_url_from(override_url: Option<&str>) -> String` (the override if `Some`, else `https://api.github.com/repos/{REPO}/releases/latest`) and `update::latest_url() -> String` = `latest_url_from(std::env::var("MFX_UPDATE_URL").ok().as_deref())`.
- Produces: `update::asset_name(v: &Version) -> String` (`mfx-v{v}-universal.zip`).
- Produces: `update::Release { pub version: Version, pub tag: String, pub zip_url: String, pub page_url: String }`.
- Produces: `update::parse_latest(json: &str) -> Result<Release, String>` and `update::is_offer(running: &Version, latest: &Release) -> bool` (newer and not a pre-release).

- [ ] **Step 1: Write the failing tests:**

```rust
#[test] fn parse_latest_reads_tag_and_asset() {
    let r = parse_latest(include_str!("fixtures/release-latest.json")).unwrap();
    assert_eq!(r.tag, "v0.2.0"); assert_eq!(r.version, Version::parse("0.2.0").unwrap());
    assert!(r.zip_url.ends_with("/mfx-v0.2.0-universal.zip"));
}
#[test] fn parse_latest_requires_matching_asset() { /* fixture with assets renamed → Err */ }
#[test] fn parse_latest_rejects_error_body() { assert!(parse_latest(include_str!("fixtures/rate-limited.json")).is_err()); }
#[test] fn parse_latest_rejects_garbage() { assert!(parse_latest("<html>").is_err()); }
#[test] fn offers_only_newer_finals() {
    // running 0.1.0: latest 0.2.0 → true; 0.1.0 → false; 0.0.9 → false; 0.2.0-rc.1 (prerelease) → false
    // running 0.2.0-rc.1: latest 0.2.0 → true
}
#[test] fn update_url_override() { /* latest_url_from(Some("http://x/l.json")) == "http://x/l.json"; latest_url_from(None) ends with "/repos/jeffujioka/mouse-fix-rs/releases/latest" */ }
```

- [ ] **Step 2:** `cargo test update::` → FAIL.
- [ ] **Step 3:** Implement with `serde_json` derive structs. A `draft` or `prerelease` flag set to true → `Err`.
- [ ] **Step 4:** `cargo test` → PASS.
- [ ] **Step 5:** Commit: `feat: release lookup and update decision`.

### Task 10: Applying an update

**Files:**
- Create: `build.rs`, `src/macos/updater.rs`
- Modify: `src/macos/mod.rs`, `src/macos/ffi.rs` (add `renamex_np` and `RENAME_SWAP = 0x2` if `libc` lacks them)

**Interfaces:**
- Consumes: `update::Release`, `paths::bundle_of` and `paths::BUNDLE_ID`.
- Produces: `updater::DESIGNATED_REQUIREMENT: Option<&str> = option_env!("MFX_DESIGNATED_REQUIREMENT")`. `build.rs` sets it from `packaging/designated-requirement.txt` (trimmed) unless the variable is already in the build environment, with `cargo:rerun-if-changed` on the file and `cargo:rerun-if-env-changed=MFX_DESIGNATED_REQUIREMENT`.
- Produces: `updater::fetch(url: &str) -> Result<String, String>` (`/usr/bin/curl -fsSL --max-time 30 -H "Accept: application/vnd.github+json"`).
- Produces: `updater::can_replace(bundle: &Path) -> bool` (the parent is writable by `access(W_OK)` and the bundle's owner uid is `getuid()`).
- Produces: `updater::verify(app: &Path, requirement: &str, expected_version: &Version) -> Result<(), String>` (`codesign --verify --deep --strict -R=<req>` plus `plutil -extract CFBundleShortVersionString raw -o - <app>/Contents/Info.plist`).
- Produces: `updater::swap(a: &Path, b: &Path) -> std::io::Result<()>` (`renamex_np(RENAME_SWAP)`).
- Produces: `updater::apply(release: &Release, bundle: &Path, requirement: &str) -> Result<(), String>`. Steps:
  1. `can_replace`.
  2. Download into `<install dir>/.mfx-update/mfx.zip`.
  3. `ditto -x -k` into `<install dir>/.mfx-update/`.
  4. `verify` `.mfx-update/mfx.app`.
  5. `swap` it with the bundle.
  6. Remove `.mfx-update/`.

  On any error before the swap it removes `.mfx-update/` and returns `Err`.
- Produces: `updater::relaunch_after_exit(bundle: &Path) -> std::io::Result<()>`. It spawns `/bin/sh -c 'while kill -0 $0; do sleep 0.2; done; open "$1"' <pid> <bundle>` in its own process group.

- [ ] **Step 1: Write the failing tests** (temp dirs; `ditto` and `codesign` exist on every Mac):

```rust
#[test] fn swap_exchanges_directories() { /* a/marker=A, b/marker=B → after swap a/marker=B, b/marker=A */ }
#[test] fn verify_rejects_wrong_signature() {
    // fake bundle: Contents/MacOS/mfx (copy of /usr/bin/true), Info.plist CFBundleIdentifier=com.jeffujioka.mfx,
    // CFBundleShortVersionString=0.2.0; `codesign -s - --identifier com.jeffujioka.mfx`
    // verify(.., r#"identifier "com.jeffujioka.mfx" and certificate leaf = H"0000000000000000000000000000000000000000""#, 0.2.0) → Err
}
#[test] fn verify_rejects_version_mismatch() { /* same bundle, requirement `identifier "com.jeffujioka.mfx"`, expected 0.3.0 → Err; expected 0.2.0 → Ok */ }
#[test] fn apply_with_corrupt_zip_leaves_bundle_untouched() {
    // install dir with mfx.app (marker file); local HTTP not needed: Release.zip_url = "file://<path to garbage.zip>"
    // apply → Err; mfx.app/marker unchanged; no .mfx-update left
}
#[test] fn apply_refuses_unwritable_install_dir() { /* chmod 0555 parent → Err mentioning the Release page; restore perms */ }
```

- [ ] **Step 2:** `cargo test updater::` → FAIL.
- [ ] **Step 3:** Implement `build.rs` and `updater.rs` (curl accepts `file://`, which the tests use).
- [ ] **Step 4:** `cargo test` → PASS.
- [ ] **Step 5:** Commit: `feat: download, verify and swap updates`.

### Task 11: Update menu and daily check

**Files:**
- Modify: `src/macos/app.rs` (`UserEvent` gains `UpdateChecked(Result<Option<update::Release>, String>)` and `UpdateFailed(String)`; `Items` gains `check_updates` and `update`; `App` gains `menu: Menu`, `check_updates: bool`, `last_check: Option<std::time::Instant>` and `offer: Option<update::Release>`)

**Interfaces:**
- Consumes: `update::{latest_url, parse_latest, is_offer, Release}`, `version::Version::current()`, `updater::{fetch, apply, relaunch_after_exit, DESIGNATED_REQUIREMENT}`, `paths::bundle_of`, `Config.check_updates`.

- [ ] **Step 1: Menu.** "Check for Updates…" sits above "Quit mfx". "Update to vX.Y.Z…" is inserted directly below the version line while there is an offer.
  - A manual check with no update → label `Check for Updates… (up to date)`.
  - A failed check → label `Check for Updates… (check failed)`, with the error logged.
- [ ] **Step 2: Daily check.** On `Tick`, if `check_updates` and (`last_check` is `None` or older than 24 h), spawn a thread: `fetch(latest_url())`, then `parse_latest`, then `is_offer`. Send the result as `UserEvent::UpdateChecked`. Set `last_check` when the check starts. No network work runs on the main thread.
- [ ] **Step 3: Apply on click.**
  - If `DESIGNATED_REQUIREMENT` is `None` or `bundle_of(current_exe)` is `None`, open the Release page (`open <page_url>`) and log why.
  - Otherwise kill `settings_child`, then run `apply` on a thread.
  - On `Ok`: `relaunch_after_exit(bundle)`, then `el.exit()`.
  - On `Err`: `UpdateFailed` → log it and open `page_url`.
- [ ] **Step 4: Smoke test.**
  1. Write the test cert's DR to `packaging/designated-requirement.txt`. Don't commit it.
  2. Build and install the bundle at version `0.1.0`.
  3. Bump `Cargo.toml` to `0.1.1` and build a signed zip via `scripts/release.sh --dry-run build package`.
  4. Serve a `latest.json` (tag `v0.1.1`, asset URL → the zip) with `python3 -m http.server`.
  5. Run the app with `MFX_UPDATE_URL=http://127.0.0.1:8000/latest.json open --env MFX_UPDATE_URL=… /Applications/mfx.app`, then click "Check for Updates…" and "Update to v0.1.1…".

  Expected: mfx relaunches, the menu shows `mfx 0.1.1`, Accessibility is still granted.

  Repeat with an ad-hoc-signed zip. Expected: refused, the Release page opens, `/Applications/mfx.app` still shows `0.1.0`.
- [ ] **Step 5:** Revert the version bump and remove the uncommitted DR file. Commit: `feat: update menu and daily check`.

### Task 12: Settings: General pane and read-only invalid config

**Files:**
- Modify: `src/settings.rs:36-48` (initial state), `:92-118` (`Pane::General`, first in `ALL`, title "General", tint `Color32::from_rgb(142, 142, 147)`), `:120-174` (state, focus handling, read-only), `:212-234` (footer)

**Interfaces:**
- Consumes: `login_item::{status, set_enabled, open_system_settings, Status}` and `ConfigFile.updates`.
- Produces: `settings::initial_state(read: std::io::Result<String>) -> Result<(ConfigFile, Option<String>), String>`, where the second value is the read-only error.

- [ ] **Step 1: Write the failing test:**

```rust
#[test] fn initial_state_cases() {
    let (f, ro) = initial_state(Ok("[scroll]\nsmooth = false".into())).unwrap(); assert!(!f.scroll.smooth); assert!(ro.is_none());
    let (_, ro) = initial_state(Ok("[scroll]\nbogus = 1".into())).unwrap(); assert!(ro.unwrap().contains("bogus"));
    let (f, ro) = initial_state(Err(std::io::ErrorKind::NotFound.into())).unwrap(); assert_eq!(f, default_file()); assert!(ro.is_none());
    assert!(initial_state(Err(std::io::ErrorKind::PermissionDenied.into())).is_err());
}
```

- [ ] **Step 2:** `cargo test settings::` → FAIL.
- [ ] **Step 3: Read-only mode.** Implement `initial_state` and use it in `run`. While read-only:
  - `ui()` never calls `save()`.
  - The panes are drawn inside `ui.add_enabled_ui(false, …)`.
  - The footer shows `⚠ <error>. Fix the file to edit settings here.` plus the existing "Reveal in Finder" action.
- [ ] **Step 4: General pane.**
  - Toggle row "Start at login", bound to `status()`. Turning it on or off calls `set_enabled`; an error goes to `notice`.
  - When the status is `RequiresApproval`, the toggle shows off, with help text `Blocked in System Settings.` and a button "Open Login Items…" that calls `open_system_settings()`.
  - Re-read `status()` when the window opens and whenever the viewport's `focused` flag changes from false to true.
  - Toggle row "Check for updates automatically", bound to `file.updates.check_automatically` and saved like every other setting.
- [ ] **Step 5:** Verify:
  - `cargo test` passes.
  - Add `bogus = 1` under `[scroll]` in the config, open Settings, click around. `shasum` of the file is unchanged before and after.
  - With the bundle installed: switch mfx off in System Settings → Login Items, focus the Settings window. The toggle flips off within one focus change.
- [ ] **Step 6:** Commit: `feat: General settings pane, read-only on invalid config`.

### Task 13: install.sh

**Files:**
- Create: `install.sh` (repo root, executable), `tests/install_test.sh` (executable)

**Interfaces:**
- CLI: `install.sh [--version vX.Y.Z] [--no-launch]` and `install.sh --uninstall [--purge]`.
- Environment:
  - `MFX_INSTALL_ZIP=<path>`
  - `MFX_INSTALL_DIR=<dir>` (testing: overrides the install-dir choice)
  - `MFX_DESIGNATED_REQUIREMENT` (testing: overrides the pinned DR)
  - `MFX_TEST_SYSTEM_APPS=<dir>` (testing: stands in for `/Applications` in the install-dir lookup)
- Constants at the top of the script: `REPO="jeffujioka/mouse-fix-rs"`, `BUNDLE_ID="com.jeffujioka.mfx"`, `DR='<content of packaging/designated-requirement.txt>'`. An empty `DR` with no override → exit 1 `install.sh has no pinned release certificate`.

- [ ] **Step 1: Write the failing test script.** Each test runs with `HOME=$(mktemp -d)` and `MFX_INSTALL_DIR=$HOME/Apps` and builds fake zips. A fake zip contains `mfx.app` whose `Contents/MacOS/mfx` is a shell script answering `login-item off` with exit 0. It is signed ad-hoc with `--identifier com.jeffujioka.mfx` and named `mfx-v0.2.0-universal.zip`. `MFX_DESIGNATED_REQUIREMENT='identifier "com.jeffujioka.mfx"'`. Cases:
  1. `install_fresh`: `--no-launch` → `$HOME/Apps/mfx.app` exists; `$HOME/.local/bin/mfx` symlink points to it; stdout warns that `~/.local/bin` isn't on PATH when it's removed from `PATH`.
  2. `install_rejects_bad_signature`: requirement `identifier "other"` → exit ≠ 0; the previously installed app is unchanged (marker file intact); no staging dir left in `$HOME/Apps`.
  3. `install_rejects_version_mismatch`: zip named `mfx-v0.3.0-universal.zip` containing 0.2.0 → exit ≠ 0.
  4. `reinstall_reuses_location`: install into `$HOME/Apps`, then rerun with `MFX_INSTALL_DIR` unset but `HOME` the same and `/Applications` faked as unwritable via `MFX_TEST_SYSTEM_APPS=$HOME/ro` (mode 0555). This tests the existing-copy rule; `MFX_TEST_SYSTEM_APPS` replaces `/Applications` in the lookup. Expected: updated in place, never a second copy.
  5. `quarantine_stripped`: `xattr -w com.apple.quarantine 0081;0;test; zip` → the installed app has no `com.apple.quarantine`.
  6. `uninstall_keeps_config`: `--uninstall` removes the app, symlink, `Application Support/mfx`, `Library/Logs/mfx.log`; `~/.config/mfx/config.toml` stays.
  7. `uninstall_purge`: `--uninstall --purge` also removes `~/.config/mfx`.
  8. `dr_matches_packaging`: when `packaging/designated-requirement.txt` exists, `install.sh`'s `DR` equals its trimmed content.
- [ ] **Step 2:** `tests/install_test.sh` → FAIL (no `install.sh`).
- [ ] **Step 3: Implement `install.sh`** with `set -euo pipefail`:
  - **Install dir:** `MFX_INSTALL_DIR`, else an existing `mfx.app` in `${MFX_TEST_SYSTEM_APPS:-/Applications}` or `~/Applications`. Stop if that location isn't writable. Otherwise `/Applications` if writable, else `~/Applications`.
  - **Download:** `curl -fsSL` the Release API (`/releases/latest`, or `/releases/tags/<tag>` with `--version`) and pick the asset by name. Download it plus `.sha256` and check with `shasum -a 256 -c`. With `MFX_INSTALL_ZIP`, take the version from the file name and strip quarantine.
  - **Unpack and verify:** `ditto -x -k` into `<dir>/.mfx-update/`, then `codesign --verify --deep --strict -R="$DR"` and the `plutil` version check.
  - **Swap:** quit with `pkill -f "<dir>/mfx.app/Contents/MacOS/mfx"` and wait up to 5 s, then `mv` the old app to `.mfx-update/old.app`, `mv` the new one into place, and `rm -rf .mfx-update`.
  - **Finish:** create `~/.local/bin` and `ln -sfn` the symlink; warn when it isn't on `PATH`; `open` the app unless `--no-launch`.
  - **Uninstall:** as in the spec's `--uninstall` steps, with `tccutil` failure ignored and printed.
- [ ] **Step 4:** `tests/install_test.sh` → all pass.
- [ ] **Step 5:** Smoke test on the real system with the Task 11 signed zip: `MFX_INSTALL_ZIP=dist/mfx-v0.1.0-universal.zip MFX_DESIGNATED_REQUIREMENT="$(cat packaging/designated-requirement.txt)" ./install.sh`. Expected: `/Applications/mfx.app` replaced, mfx running, `mfx --version` through the symlink prints `mfx 0.1.0`.
- [ ] **Step 6:** Commit: `feat: install.sh (install, update, uninstall)`.

### Task 14: Docs

**Files:**
- Modify: `README.md` (Build, Run, Start at login, Configure), `CHANGELOG.md` (`[Unreleased]`), `docs/superpowers/specs/2026-10-28-release-install-update-design.md` (staging is the `.mfx-update/` directory; spike outcome for item 3)

- [ ] **Step 1: README.**
  - **Install:** the `curl -fsSL https://raw.githubusercontent.com/jeffujioka/mouse-fix-rs/HEAD/install.sh | bash` line; `--version`; the zip fallback ("Open Anyway", or `xattr -dr com.apple.quarantine /Applications/mfx.app`).
  - **Start at login:** the Settings toggle, Login Items, and the "+" known limit if spike item 3 failed. Remove the LaunchAgent section and the crash-restart claim.
  - **Updates:** "Check for Updates…", the daily-check setting, the log at `~/Library/Logs/mfx.log`.
  - **Uninstall:** `--uninstall [--purge]`.
  - **Development:** `scripts/build.sh` and the terminal permission caveat.
- [ ] **Step 2:** CHANGELOG `[Unreleased]` → `Added`: app bundle, install script, updater, Start at login toggle, single instance, `--version`, log file. `Changed`: Settings won't overwrite an invalid config. `Removed`: `mfx launch-agent`.
- [ ] **Step 3:** Verify: `cargo test && tests/install_test.sh && scripts/release.sh --dry-run build package` → all pass.
- [ ] **Step 4:** Commit: `docs: install, update and Start at login`.

## Before the first Release (maintainer, not a task)

Create the release certificate (20 years, legacy PKCS#12). Commit `packaging/designated-requirement.txt` and the same string in `install.sh`'s `DR` (`dr_matches_packaging` enforces it). Set the final `REPO` in `src/update.rs`, `install.sh` and the README if the repo is renamed. Then follow `RELEASING.md`.

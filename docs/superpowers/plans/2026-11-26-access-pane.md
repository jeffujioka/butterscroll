# butterscroll Access pane, direct Grant Access and warning icon: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** butterscroll shows plainly when it lacks Access: a warning menu-bar icon, Settings opening by itself on a new Access pane with live status and steps, and Grant Access… landing on the right System Settings screen.

**Architecture:**
- A new macOS module `src/macos/access.rs` holds the pure decisions, with unit tests:
  - which System Settings link fits the macOS version;
  - when to open Settings by itself.
- The menu app (`src/macos/app.rs`) feeds it the tap state once a second and acts on its answers: the icon, opening Settings, the status line.
- The Settings window (`src/settings.rs`) gets `Pane::Access`. It polls `tap::can_create()` at most once a second, through a small throttle that has its own test.
- Screenshots come last, once the maintainer has captured them.

**Tech Stack:** Rust 2021, winit 0.30, tray-icon 0.26.1, eframe/egui 0.35, libc 0.2; Task 6 adds egui_extras 0.35 and image 0.25 (PNG only).

**Spec:** `docs/superpowers/specs/2026-11-26-access-pane-design.md`

## Global Constraints

- **Wording:** UI text and log lines say "Access", never "permission" (`GLOSSARY.md`).
  - The menu status line reads exactly `Needs access`.
  - The pane title is `Access`.
  - Status labels are `✓ Allowed` and `✕ Not allowed`.
- **"Has Access"** means `tap::can_create()` returns true. Nothing else decides it.
- **Override link (macOS 27.3 and later):** `x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension.privacy-autonomous-control`.
- **Accessibility link (before 27.3):** `x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility`.
- **Settings CLI:** `butterscroll settings [--config PATH] [--pane access]`. Only `access` is accepted; any other value is an error.
- **Branch:** `feat/access-pane`, based on `fix/dead-event-tap`. `tap::is_alive()` and `tap::uninstall()` come from that branch.
- **Code style:**
  - Errors are `Result<_, String>`, logged as `eprintln!("butterscroll: …")`.
  - Tests are inline `#[cfg(test)] mod tests`.
  - macOS-only calls in `settings.rs` and `main.rs` sit behind `#[cfg(target_os = "macos")]`.
- **Checks after each task:**
  - `cargo test --bin butterscroll` passes;
  - `cargo clippy --release` adds no warning beyond the two existing ones (`bundle_of` unused, derivable `impl`).
- **Running build:** `BUTTERSCROLL_SIGN_ID="Apple Development: jefferson.fujioka@gmail.com (75RK5BJ4MH)" ./scripts/build.sh`, then `launchctl kickstart -k gui/$(id -u)/com.jeffujioka.butterscroll`. Never ship an ad-hoc build to the launchd job: it drops the grant.

## Review Focus

1. **egui repaints at frame rate while the pointer moves.**
   - `can_create()` must still run at most once a second, or the probe itself creates dozens of taps a second.
   - Pinned in Task 4: `probe_checks_at_most_once_a_second`.
2. **A tap that macOS disables again right after each reinstall (flapping)** must not pop Settings up every few seconds.
   - Recovery counts only after 10 consecutive ticks with Access. This is the plan's reading of "after Access worked again in between".
   - Pinned in Task 2: `flapping_tap_opens_once`.
3. **Settings already open when Access fails:** the failure counts as handled, and no spawn loop follows.
   - Pinned in Task 2: `missing_access_opens_once_until_recovered`.
   - Covered in Task 5 by `settings_child` reuse.
4. **The macOS version string is a beta, a patch release or unreadable** (`27.3`, `27.3.1`, `27.3 beta`, empty).
   - An unreadable version picks the Override link, because the shipping macOS is 27.3 or later.
   - Pinned in Task 2: `parses_product_versions` and `unknown_version_uses_override_link`.
5. **The 27.3 boundary:** 27.2 and 26.x get the Accessibility link; 27.3, 27.4 and 28.0 get the Override link.
   - Pinned in Task 2: `grant_url_by_version`.

---

### Task 0: Maintainer checks (blocks Task 4's Input Monitoring row and Task 7's wording)

**Files:**
- Modify: `docs/superpowers/specs/2026-11-26-access-pane-design.md`, adding a `## Findings` section at the end.

- [ ] **Step 1:** Ask the maintainer to switch butterscroll **off** under System Settings → Privacy & Security → Input Monitoring.
- [ ] **Step 2:** Restart the job with `launchctl kickstart -k gui/$(id -u)/com.jeffujioka.butterscroll` and wait 3 s.
- [ ] **Step 3:** Check the result.
  - Run `tail -3 ~/Library/Logs/butterscroll.log`.
  - Run the tap listing from `docs/troubleshooting/macos-27-3-permission.md` → Diagnose step 3.
  - Ask the maintainer whether a middle-button drag scrolls.
  - Expected, one of two:
    - **(a)** `butterscroll: running`, the tap is `e400000 true` and the drag works: Input Monitoring is not needed.
    - **(b)** It is needed.
- [ ] **Step 4:** Ask the maintainer to switch Input Monitoring back on if the result was (b).
- [ ] **Step 5:** Write the result in `## Findings`, then commit: `git commit -am "docs(spec): record whether Input Monitoring is needed"`.
- [ ] **Step 6:** If the result was **(b)**, stop and add a task before Task 4 for the second status row:
  - status from `IOHIDCheckAccess(kIOHIDRequestTypeListenEvent)`;
  - link `x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent`.

  If it was (a), Task 7 removes Input Monitoring from the docs and from `permission_hint`.

### Task 1: A tap counts only if it is enabled

**Files:**
- Modify: `src/macos/tap.rs`, in `create_port` (around lines 64-81 on `fix/dead-event-tap`)

**Interfaces:**
- Produces: `can_create()` and `install()` keep their signatures. Both now return false when macOS creates the tap but leaves it disabled.

- [ ] **Step 1:** In `create_port`, after `CGEventTapCreate`, check a non-null port.
  - When `CGEventTapIsEnabled(port)` is false, call `CFMachPortInvalidate` and `CFRelease` on it and return null.
  - Update the doc comment: "null while macOS denies it or leaves it disabled".
- [ ] **Step 2:** Run `cargo test --bin butterscroll && cargo build --release`. Expected: PASS, and the build succeeds.
  - This is an FFI-only change with no unit test; Step 3 checks it on the running app.
- [ ] **Step 3:** Run the running build, then `tail -2 ~/Library/Logs/butterscroll.log`. Expected: `butterscroll: running (config: …)`.
- [ ] **Step 4:** Commit: `git commit -am "fix(tap): treat a tap macOS leaves disabled as no Access"`.

### Task 2: Access decisions: Settings link and when to open Settings

**Files:**
- Create: `src/macos/access.rs`
- Modify: `src/macos/mod.rs` (add `pub mod access;`)

**Interfaces:**
- Produces:
  - `pub const OVERRIDE_URL: &str` and `pub const ACCESSIBILITY_URL: &str`, with the values from Global Constraints.
  - `pub fn parse_os_version(s: &str) -> Option<(u32, u32)>`: major and minor from a `kern.osproductversion` string.
  - `pub fn grant_url_for(os: Option<(u32, u32)>) -> &'static str`.
  - `pub fn grant_url() -> &'static str`: reads `kern.osproductversion` with `libc::sysctlbyname` and calls `grant_url_for`.
  - `pub const RECOVERY_TICKS: u32 = 10`.
  - `pub struct AccessWatch`, with `pub fn new() -> Self` and `pub fn update(&mut self, has_access: bool) -> bool`. It is called once per tick and returns true exactly when Settings should open on the Access pane.

- [ ] **Step 1: Write the failing tests** in `src/macos/access.rs`:

```rust
#[test]
fn parses_product_versions() {
    assert_eq!(parse_os_version("27.3"), Some((27, 3)));
    assert_eq!(parse_os_version("27.3.1"), Some((27, 3)));
    assert_eq!(parse_os_version("28"), Some((28, 0)));
    assert_eq!(parse_os_version("27.3 beta"), Some((27, 3)));
    assert_eq!(parse_os_version(""), None);
    assert_eq!(parse_os_version("abc"), None);
}

#[test]
fn grant_url_by_version() {
    assert_eq!(grant_url_for(Some((26, 5))), ACCESSIBILITY_URL);
    assert_eq!(grant_url_for(Some((27, 2))), ACCESSIBILITY_URL);
    assert_eq!(grant_url_for(Some((27, 3))), OVERRIDE_URL);
    assert_eq!(grant_url_for(Some((27, 4))), OVERRIDE_URL);
    assert_eq!(grant_url_for(Some((28, 0))), OVERRIDE_URL);
}

#[test]
fn unknown_version_uses_override_link() {
    assert_eq!(grant_url_for(None), OVERRIDE_URL);
}

#[test]
fn missing_access_at_launch_opens_once() {
    let mut w = AccessWatch::new();
    assert!(w.update(false));
    assert!(!w.update(false));
    assert!(!w.update(false));
}

#[test]
fn access_at_launch_never_opens() {
    let mut w = AccessWatch::new();
    for _ in 0..20 { assert!(!w.update(true)); }
}

#[test]
fn missing_access_opens_once_until_recovered() {
    let mut w = AccessWatch::new();
    for _ in 0..RECOVERY_TICKS { w.update(true); }
    assert!(w.update(false));          // the tap died
    assert!(!w.update(false));         // retries, same failure
    for _ in 0..RECOVERY_TICKS { assert!(!w.update(true)); }
    assert!(w.update(false));          // a new failure after a real recovery
}

#[test]
fn flapping_tap_opens_once() {
    let mut w = AccessWatch::new();
    assert!(w.update(false));
    for _ in 0..5 {
        for _ in 0..RECOVERY_TICKS - 1 { w.update(true); }
        assert!(!w.update(false));     // never stayed up long enough to count as recovered
    }
}
```

- [ ] **Step 2:** Run `cargo test --bin butterscroll macos::access`. Expected: FAIL to compile (`cannot find function parse_os_version`).
- [ ] **Step 3:** Implement the interfaces above.
  - `AccessWatch` keeps two fields: `reported: bool`, and `ticks_with_access: u32`.
    - `update(false)`: reset `ticks_with_access` to 0. Return true only if `!reported`, then set `reported`.
    - `update(true)`: increment `ticks_with_access`. When it reaches `RECOVERY_TICKS`, clear `reported`. Return false.
  - `parse_os_version` takes the leading run of digits and dots, then splits on `.`. A missing minor is 0.
- [ ] **Step 4:** Run `cargo test --bin butterscroll macos::access`. Expected: 7 passed.
- [ ] **Step 5:** Commit: `git add src/macos/access.rs src/macos/mod.rs && git commit -m "feat(access): pick the Settings link by macOS version and decide when to open Settings"`.

### Task 3: Settings CLI `--pane access`

**Files:**
- Modify: `src/main.rs`:
  - `USAGE`: the settings line becomes `butterscroll settings [--config PATH] [--pane access]  open the settings window`;
  - `struct Args`: add `pane: Option<String>`;
  - `parse_args`: `--pane` needs a value;
  - `real_main`: the `"settings"` arm calls `settings::run(config_path(args.config)?, args.pane.as_deref())`.
- Modify: `src/settings.rs`: `run`, `enum Pane`, `Pane::ALL`, `title`, `tint`, `paint_icon`, the `match self.pane` in `ui`.

**Interfaces:**
- Produces:
  - `settings::run(path: PathBuf, pane: Option<&str>) -> Result<(), String>`. `None` starts on `Pane::Scrolling`. `Some("access")` starts on `Pane::Access`. Anything else returns `Err("unknown pane \"<value>\" (expected: access)")` before the window opens.
  - `Pane::Access`, last in `Pane::ALL` (`[Pane; 4]`), titled `Access`, with the tint `Color32::from_rgb(0, 122, 255)` (system blue tile, white shield or lock glyph in `paint_icon`).
  - `Pane::from_arg(s: &str) -> Option<Pane>`.
  - A placeholder `fn access_pane(ui: &mut Ui)` that shows only the heading for now. Task 4 replaces it.

- [ ] **Step 1: Write the failing test** in `src/settings.rs` tests:

```rust
#[test]
fn pane_from_arg_accepts_only_access() {
    assert!(matches!(Pane::from_arg("access"), Some(Pane::Access)));
    assert!(Pane::from_arg("scrolling").is_none());
    assert!(Pane::from_arg("").is_none());
}
```

- [ ] **Step 2:** Run `cargo test --bin butterscroll pane_from_arg`. Expected: FAIL to compile (no `Pane::Access`).
- [ ] **Step 3:** Implement the interfaces above.
- [ ] **Step 4:** Run `cargo test --bin butterscroll`. Expected: PASS.
  - Run `cargo run --release -- settings --pane nope`. Expected: exit code 1, with `unknown pane "nope" (expected: access)`.
- [ ] **Step 5:** Commit: `git commit -am "feat(settings): add the Access pane and --pane access"`.

### Task 4: Access pane content and live status

**Files:**
- Modify: `src/settings.rs`: `SettingsApp`, `ui` (hide the footer on Access), `sidebar` and `sidebar_item` (red dot), `access_pane`.

**Interfaces:**
- Consumes: `crate::macos::tap::can_create()`; `crate::macos::access::grant_url()` (Task 2); `Pane::Access` (Task 3).
- Produces:
  - `const PROBE_INTERVAL: Duration = Duration::from_secs(1)`.
  - `#[derive(Default)] struct AccessProbe { checked: Option<Instant>, allowed: bool }`, with `fn refresh(&mut self, now: Instant, check: impl FnOnce() -> bool) -> bool`. It calls `check` only when the last check is `PROBE_INTERVAL` or more ago, and returns `allowed`.
  - `SettingsApp` gets the field `access: AccessProbe`.
  - `sidebar_item(ui, pane, selected, badge: bool)`.

- [ ] **Step 1: Write the failing test**:

```rust
#[test]
fn probe_checks_at_most_once_a_second() {
    let t0 = Instant::now();
    let mut probe = AccessProbe::default();
    let mut calls = 0;
    assert!(!probe.refresh(t0, || { calls += 1; false }));
    for ms in [1, 16, 500, 999] {
        probe.refresh(t0 + Duration::from_millis(ms), || { calls += 1; true });
    }
    assert_eq!(calls, 1);
    assert!(probe.refresh(t0 + Duration::from_millis(1000), || { calls += 1; true }));
    assert_eq!(calls, 2);
}
```

- [ ] **Step 2:** Run `cargo test --bin butterscroll probe_checks`. Expected: FAIL to compile (no `AccessProbe`).
- [ ] **Step 3:** Implement `AccessProbe`.
  - In `SettingsApp::ui`, call `self.access.refresh(Instant::now(), can_create)` every frame. On non-macOS, `check` is `|| false`.
  - Then call `ui.ctx().request_repaint_after(PROBE_INTERVAL)` so the status updates without input.
- [ ] **Step 4:** Implement `access_pane(ui, allowed: bool, exe: &Path)` with the existing `card`, `section`, `row` and `footnote` helpers. Copy exactly:
  - **Status card:**
    - row label `System & App Protection Override`;
    - help `butterscroll needs this to smooth scrolling and remap buttons.`;
    - status `✓ Allowed` in green `Color32::from_rgb(52, 199, 89)`, or `✕ Not allowed` in `p.danger`;
    - button `Open System Settings`, which runs `/usr/bin/open <grant_url()>`, the same way as the "Show Config File in Finder" link.
  - **Steps:**
    - section `How to allow butterscroll` when not allowed. When allowed, the steps sit in a collapsed `egui::CollapsingHeader` titled `How to allow butterscroll again`.
    - `1. Click + below the list.`
    - `2. Press Cmd-Shift-G, paste the path below, press Return, then click Add.`
    - a monospace row with `exe` in full and a `Copy` button calling `ui.ctx().copy_text(..)`;
    - `3. Choose Override System Protections.`
    - `4. Acknowledge each item, click Allow System & App Protection Override, then authenticate.`
  - **Footnote:** `butterscroll notices the change within a second. Opening + again and cancelling can switch Access off; see docs/troubleshooting/macos-27-3-permission.md.`
  - `exe` is `std::env::current_exe()` canonicalized, computed once in `run`.
- [ ] **Step 5:** Skip the footer panel while `self.pane == Pane::Access`.
  - Pass `badge = !self.access.allowed` for `Pane::Access` only.
  - The badge is a filled circle of radius 4 in `p.danger`, at the item's right edge minus 12.
- [ ] **Step 6:** Run `cargo test --bin butterscroll`. Expected: PASS.
  - Run `cargo run --release -- settings --pane access` from a terminal. Expected: the pane shows the terminal's status, and the steps and Copy button work.
  - With that window open and the pointer moving over it, scroll in another app for 30 s. Expected: no stutter compared with Settings closed. If there is stutter, raise `PROBE_INTERVAL` or probe only on window focus, and note it in the spec's `## Findings`.
- [ ] **Step 7:** Commit: `git commit -am "feat(settings): show live Access status and the steps to grant it"`.

### Task 5: Menu app: direct link, warning icon, auto-open, wording

**Files:**
- Modify: `src/macos/app.rs`:
  - remove `PRIVACY_PANE`;
  - `App` fields;
  - `refresh_menu`;
  - `open_settings`;
  - `handle_menu` (the grant and settings arms);
  - `build_tray`;
  - `new_events`;
  - `user_event`;
  - `mouse_icon_rgba`.

**Interfaces:**
- Consumes: `access::{grant_url, AccessWatch}` (Task 2); `butterscroll settings --pane access` (Task 3).
- Produces:
  - `fn mouse_icon_rgba(size: u32, warning: bool) -> Vec<u8>`.
  - `App` gets `access_watch: AccessWatch` and `icon_warning: bool`.
  - `fn open_settings(&mut self, access_pane: bool)`.
  - `fn on_access(&mut self, has_access: bool)`.

- [ ] **Step 1:** `mouse_icon_rgba(size, true)` draws the same mouse plus a diagonal slash.
  - The slash runs from top-right to bottom-left at stroke `s * 0.075`, over a clear gap of 1.5 strokes either side so it reads at 18 pt.
  - The icon stays alpha-only, as a template.
- [ ] **Step 2:** `on_access(has_access)`:
  - If `icon_warning != !has_access`, call `tray.set_icon_with_as_template(Some(icon), true)` with `mouse_icon_rgba(ICON_PX, !has_access)` and store the new state.
  - Then, if `self.access_watch.update(has_access)`, log `butterscroll: Access missing; opening Settings` and call `self.open_settings(true)`.
- [ ] **Step 3:** Call `self.on_access(self.tap_installed)`:
  - at the end of the `StartCause::Init` branch;
  - at the end of every `UserEvent::Tick`, after the existing install, alive and reload logic.

  The watch then counts ticks.
- [ ] **Step 4:** `open_settings(access_pane)` adds `--pane access` when `access_pane` is true. It keeps the "already open" early return on `settings_child`.
  - The Settings… menu arm calls `self.open_settings(!self.tap_installed)`.
  - The Grant Access… arm calls `tap::request_access()`, then `open(&[access::grant_url().as_ref()])`.
- [ ] **Step 5:** Wording, exact strings:
  - `refresh_menu` shows `Needs access`;
  - `new_events` logs `butterscroll: waiting for Access (menu → Grant Access…)`;
  - the dead-tap log is `butterscroll: event tap stopped working; waiting for Access (menu → Grant Access…)`;
  - the doc comment on line 21 drops "permission".
- [ ] **Step 6:** Run `cargo test --bin butterscroll && cargo clippy --release`. Expected: PASS, no new warning.
- [ ] **Step 7: Check on the running build.**
  - Install the running build. Expected: the normal icon, and the log shows `running`.
  - Then run `codesign --force --sign - target/release/butterscroll && launchctl kickstart -k gui/$(id -u)/com.jeffujioka.butterscroll`. The ad-hoc signature has no grant.
  - Expected:
    - the slashed icon;
    - Settings opens once, on Access, showing `✕ Not allowed`;
    - the menu reads `Needs access`;
    - Grant Access… opens the "System & App Protection Override" screen.
  - Close Settings. Expected: it does not reopen within 30 s.
  - Restore with the signed running build. The maintainer re-grants if macOS asks. Expected: the normal icon, and the status turns to `✓ Allowed` in an open Settings within a second.
- [ ] **Step 8:** Commit: `git commit -am "feat(menu): open the Override pane directly, warn with the icon and open Settings when Access is missing"`.

### Task 6: Screenshots in the Access pane (blocked on the maintainer's captures)

**Files:**
- Create: `docs/assets/access/README.md` (capture checklist), and `docs/assets/access/step-1-add.png`, `step-2-go-to-folder.png`, `step-3-override.png`, `step-4-allow.png` (from the maintainer).
- Modify: `Cargo.toml`, `src/settings.rs` (`run` creation closure, `access_pane`).

**Interfaces:**
- Consumes: `access_pane` from Task 4.

- [ ] **Step 1:** Write `docs/assets/access/README.md`, the capture checklist:
  - Use a clean macOS user account or a VM, never the main account.
  - Show butterscroll-related UI only, with other apps cropped out.
  - Capture at 2x, the window only (Cmd-Shift-4, Space), one PNG per step with the file names above, each 1200 px wide at most.
  - While capturing, note whether `Grant Access…` (which calls `tap::request_access()`) already added butterscroll to the list. If it did, tell the builder: steps 1 and 2 then become "Switch butterscroll on".

  Commit it, then **stop until the PNGs are committed**.
- [ ] **Step 2:** If the maintainer reports that `request_access` already lists butterscroll, change the step text in `access_pane` and drop the two images that no longer apply. Write the finding into the spec's `## Findings`.
- [ ] **Step 3:** Add the dependencies:
  - `egui_extras = { version = "0.35", default-features = false, features = ["image"] }`;
  - `image = { version = "0.25", default-features = false, features = ["png"] }`.

  Call `egui_extras::install_image_loaders(&cc.egui_ctx)` in the creation closure of `settings::run`.
- [ ] **Step 4:** Under each step in `access_pane`, add `ui.add(egui::Image::from_bytes("bytes://access/step-N.png", include_bytes!("../docs/assets/access/step-N-….png")).max_width(ui.available_width()).corner_radius(6))`.
- [ ] **Step 5:** Run `cargo test --bin butterscroll && cargo build --release && ls -l target/release/butterscroll`. Expected: PASS, and the binary grows by under 1.5 MB, compared with the 6 977 168 bytes measured on 2026-11-26.
  - Run `cargo run --release -- settings --pane access`. Expected: each image sits under its step and scales with the window.
- [ ] **Step 6:** Commit: `git add Cargo.toml Cargo.lock docs/assets/access src/settings.rs && git commit -m "feat(settings): show screenshots for each Access step"`.

### Task 7: Docs and remaining wording

**Files:**
- Modify:
  - `src/main.rs`: `permission_hint` becomes `access_hint`, with its two call sites; the log at line 208 becomes `butterscroll: Access missing. …`. Drop Input Monitoring if Task 0 found (a).
  - `CHANGELOG.md`, `## [Unreleased]`.
  - `docs/troubleshooting/macos-27-3-permission.md`.
  - `docs/guide/install.md`: lines 15-16, 37 and 43-57.
  - `README.md`: lines 106, 138, 142 and 172.

- [ ] **Step 1:** Update `CHANGELOG.md`, `## [Unreleased]`:
  - **Added:** `Settings has an Access pane that shows whether macOS lets butterscroll work and how to allow it, step by step.`
  - **Added:** `Grant Access… opens System & App Protection Override directly.`
  - **Added:** `The menu-bar icon shows a slash, and Settings opens on Access, when butterscroll lacks Access.`
  - **Changed:** `The menu says "Needs access" instead of "Needs permission".`
  - Fold the existing dead-tap entry's "Needs permission" into "Needs access".
- [ ] **Step 2:** Update the troubleshooting doc:
  - Symptom lines quote the new log strings and `Needs access`.
  - Fix step 2 starts with "Click Grant Access… in the menu, or Open System Settings in Settings → Access", and keeps the `+` steps.
  - Link `docs/assets/access/` once Task 6 has landed.
  - Keep the file name: README and the code link to it.
- [ ] **Step 3:** In `docs/guide/install.md` and `README.md`, replace "permission" with "Access" in user-facing sentences.
  - Keep macOS's own names: "Accessibility", "Input Monitoring", "System & App Protection Override".
  - In `install.md`, the Permissions heading becomes `## Access`.
- [ ] **Step 4:** Run `grep -rn -i "permission" src README.md docs/guide docs/troubleshooting`. Expected: no hits except the troubleshooting file name and identifiers like `check_permissions`.
  - Then run `cargo test --bin butterscroll`. Expected: PASS.
- [ ] **Step 5:** Commit: `git commit -am "docs: say Access, point to the Access pane and the direct link"`.

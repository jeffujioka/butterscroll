# butterscroll: Access pane, direct Grant Access and a visible failure state

Status: design, decisions agreed in conversation on 2026-11-26; not implemented. Terms follow [GLOSSARY.md](../../../GLOSSARY.md); **Access** is defined there. Builds on branch `fix/dead-event-tap` (commits `d8f1c68`, `7468d7d`), which must merge first.

## Intent

On 2026-11-26 butterscroll stopped working while its menu still said "butterscroll is running" and "Enabled". macOS had disabled the event tap after a rebuild changed the binary's signature, and nothing told the user. `fix/dead-event-tap` makes the menu status fall back to "Needs permission" when the tap dies. That is still easy to miss: it only shows when the menu is open, and once you see it, granting Access means finding a System Settings pane that has no obvious entry point, plus a multi-step `+` flow that only the troubleshooting doc explains.

The maintainer wants three things:

- butterscroll shows plainly whether it has Access, without opening the menu.
- Grant Access… lands on the right System Settings screen directly.
- The Settings window explains, with pictures, how to grant Access.

## Goals

- When Access is missing or the tap dies, the user notices without opening the menu: the menu-bar icon changes and Settings opens on the Access pane.
- Settings has an **Access** pane that shows the live Access status and the steps to grant it, with screenshots.
- Grant Access… (menu) and the Access pane's button open System Settings → Privacy & Security → System & App Protection Override directly.
- UI text says "Access", not "permission" (glossary).

## Non-goals

- Granting Access for the user or scripting System Settings.
- An onboarding wizard or a separate setup window (option 2 in Approaches, rejected).
- Animated GIFs (option rejected; see Approaches).
- Detecting *why* macOS revoked Access (signature change, user action, MDM).
- Changes to signing or the release pipeline.

## Anti-goals (a failure even if it technically works)

- The menu-bar icon or Access pane says all is well while butterscroll intercepts nothing.
- Settings pops up repeatedly (every second, or again after the user closed it) for one and the same failure.
- The Access status check itself slows or swallows mouse events.
- Screenshots in the public repo that show other apps, accounts or company profiles from the maintainer's Mac.

## Constraints and facts

- **Deep link.** `x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension.privacy-autonomous-control` opens System Settings on the "System & App Protection Override" screen (checked on this Mac, macOS 27.3 beta, 2026-11-26: System Events reported that window title). The identifier comes from the `UTTypeIdentifier` list in `/System/Library/ExtensionKit/Extensions/SecurityPrivacyExtension.appex/Contents/Info.plist`. Today `PRIVACY_PANE` in `src/macos/app.rs:23` opens only Privacy & Security.
- **Older macOS.** Before 27.3 the grant lives under Accessibility: `x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility`.
- **The Access test.** Whether macOS lets the process create the event tap is the only reliable test (`tap::can_create`, `src/macos/tap.rs`). `AXIsProcessTrusted` ignores the Override grant (`docs/troubleshooting/macos-27-3-permission.md`). The Override grant shows up in the macOS permission (TCC) log as `kTCCServiceAccessibility … Allowed`.
- **Responsible process.** macOS checks the grant of the responsible process. The Settings window is a child process spawned via `current_exe()` from the menu app (`src/macos/app.rs`, `open_settings`), so it is checked against the `butterscroll` binary when the menu app was started by launchd. Started from a terminal, it is checked against the terminal app instead (as documented).
- **Settings window.** `src/settings.rs` is an eframe/egui window with a sidebar of panes (`Pane`, `Pane::ALL` at line 118) and a footer with Restore Defaults and save notices. It shows no images today. Its single-instance lock makes a second `butterscroll settings` exit immediately (`src/settings.rs:43-54`).
- **Menu-bar icon.** Drawn in code as a template image by `mouse_icon_rgba` (`src/macos/app.rs`), set once in `build_tray`.
- **Dead-tap detection.** On `fix/dead-event-tap`, the menu app's once-per-second `Tick` calls `tap::is_alive()`. When it returns false, it calls `tap::uninstall()`, sets `tap_installed = false` and retries `try_install_tap` every second.
- **Re-grant pitfall.** On macOS 27.3 beta 2, opening `+`, picking butterscroll again and cancelling disabled a working tap (troubleshooting doc). Capturing screenshots of the flow on the maintainer's main account risks this.
- **Input Monitoring is unproven.** The docs say it is needed, but re-adding butterscroll there did not restore it on 2026-11-26. Only the Override re-grant did.

## Approaches considered

### Where the status and steps live

1. **An Access pane in the Settings sidebar** (picked). It is always there, with a red dot while Access is missing. It matches the System Settings layout the window already copies, and it also tells you when all is well.
2. A separate setup window shown only while Access is missing, as AltTab or Rectangle do. Rejected: a second window to build, and nowhere to check the status later.
3. A banner on top of every pane while Access is missing. Rejected: nothing confirms that all is well.

### How the steps are shown

1. **Static, cropped screenshots**, one per step (picked). Clear, and cheap to replace when macOS changes the screens.
2. An animated GIF of the flow. Rejected: animation code, a larger binary, and harder to redo.
3. Text steps only. Rejected: too hard to follow for a flow with a hidden Cmd-Shift-G step.

### When the guide opens by itself

1. **At launch when Access is missing, and when a working tap stops** (picked). The failure that triggered this work was a silent one.
2. Only at launch. Rejected: a tap that dies later stays unnoticed.
3. Never. Rejected: the same reason.

### Menu-bar icon

1. **A warning version of the icon while Access is missing** (picked).
2. One icon for every state. Rejected: failures stay hidden until the menu is opened.

## Design

### Access status, one source of truth

- "Has Access" means `tap::can_create()` returns true. No other test (not `AXIsProcessTrusted`, not TCC queries) decides the status.
- *Builder's call, settled:* `can_create` (and `install`) count a tap only if `CGEventTapIsEnabled` is true right after it is created. A tap that macOS creates but leaves disabled counts as no Access. Otherwise the Access pane could say "✓ Allowed" while the menu app's tap is dead, and the menu app would loop through install, die, uninstall. Verify on the clean account whether a revoked grant yields a null port or a disabled one; the rule covers both.
- The **menu app** knows its own state: `tap_installed` after `try_install_tap`, and false again once `tap::is_alive()` fails (from `fix/dead-event-tap`). The icon, the menu status line and auto-opening follow this state.
- The **Access pane** checks `tap::can_create()` itself: once when the pane is shown, then once a second while the window is open. It doesn't depend on the menu app running.

### Menu app (`src/macos/app.rs`)

- **Grant Access…** opens the deep link from the facts above. On macOS before 27.3 it opens the Accessibility link. It keeps calling `tap::request_access()` first, as today.
- **Status line.** "Needs permission" becomes "Needs access". The log lines that say "waiting for permission" and "permission missing" become "waiting for access" and "Access missing", same structure.
- **Icon.** While `tap_installed` is false, the tray shows a warning variant of the mouse icon. It stays a template image so macOS tints it, and it should read clearly at 18 pt, for example a diagonal slash or a small `!` badge. When the tap is installed, the normal icon returns. The icon is swapped only when the state changes, not every tick.
- **Auto-open.** butterscroll spawns Settings on the Access pane:
  - once at launch, when the first `try_install_tap` fails;
  - once when `tap::is_alive()` turns false for a tap that was installed.

  It does not open again for the same failure, neither while retrying every second nor after the user closed the window. A new failure, after Access worked again in between, opens it again. *Builder's call:* the failure counts as handled once the spawn was attempted, including when Settings was already open (`settings_child` still running) and nothing new appeared. The sidebar dot is then the signal.
- Settings opened from the menu (Settings…) still opens on Scrolling, unless Access is missing at that moment. Then it opens on Access.
- At launch, the macOS prompt from `tap::request_access()` and the auto-opened Settings may both appear. *Builder's call:* keep both for now. If the clean-account check shows the prompt neither adds butterscroll to the list nor helps otherwise, drop the prompt.
- *Builder's call:* a run started from a terminal gets no special handling. The pane shows what `can_create` says for the terminal's grant, as `docs/troubleshooting/macos-27-3-permission.md` already explains. butterscroll is installed as a bare binary that launchd runs from `target/release/butterscroll` (`~/Library/LaunchAgents/com.jeffujioka.butterscroll.plist`), so the canonical `current_exe()` is the path to add.

### Settings CLI

- `butterscroll settings [--config PATH] [--pane access]`. This is an additive flag, and only `access` is needed.
- When Settings is already open, the single-instance lock makes the second process exit. The open window then stays on its current pane, which is acceptable: the red dot in the sidebar still shows. Bringing the open window to front and switching pane is out of scope.

### Access pane (`src/settings.rs`)

- A new `Pane::Access` titled "Access", last in the sidebar, with its own tint and icon in the style of the existing ones. While Access is missing, the sidebar item shows a red dot.
- Sketch, top to bottom:

  ```
  Access
  ┌──────────────────────────────────────────────────────────────┐
  │ System & App Protection Override        ✕ Not allowed        │
  │ butterscroll needs this to smooth scrolling and remap        │
  │ buttons.                          [Open System Settings]     │
  └──────────────────────────────────────────────────────────────┘
  How to allow butterscroll
  ┌──────────────────────────────────────────────────────────────┐
  │ 1. Click + below the list.                      [screenshot] │
  │ 2. Press Cmd-Shift-G, paste the path, press Return, Add.     │
  │    /full/path/to/butterscroll   [Copy]          [screenshot] │
  │ 3. Choose Override System Protections.          [screenshot] │
  │ 4. Acknowledge the items, click Allow, authenticate.         │
  │                                                 [screenshot] │
  └──────────────────────────────────────────────────────────────┘
  butterscroll notices the change within a second.
  ```

- The status row shows "✓ Allowed" (success color) or "✕ Not allowed" (danger color). It updates live while the window is open.
- The path in step 2 is `std::env::current_exe()` (canonicalized), shown in full with a Copy button, because Cmd-Shift-G needs the exact path.
- The steps show in both states. When Access is allowed, they move under a collapsed "How to allow butterscroll again" disclosure, which helps after a rebuild that drops the grant.
- The footer's Restore Defaults button and the save notices don't apply to the Access pane and are hidden there.
- A footnote links the troubleshooting doc for the pitfalls: do the `+` flow once, and look on the other display when the file picker seems missing.

### Screenshots

- Three or four PNGs, one per step, under `docs/assets/access/`. The Access pane embeds them with `include_bytes!`, and `docs/troubleshooting/macos-27-3-permission.md` and the README can link the same files.
- They show only butterscroll-related UI: the `+` button area, the Go to Folder sheet with the butterscroll path, the "Choose Apps for 'butterscroll' to Control" sheet, and the Allow step. Any list of other apps is cropped out.
- They are captured from a clean macOS user account or VM, not the maintainer's main account. That avoids both the privacy issue and the re-grant pitfall. **This is a maintainer step:** the builder prepares the slots, file names and a capture checklist. Until the PNGs exist, the pane shows the text steps without images.
- egui needs an image loader for PNGs: `egui_extras` with its image support and the `image` crate limited to PNG. Keep the added dependency and binary size small.

### Docs

- `CHANGELOG.md` `## [Unreleased]`: under Added, the Access pane, the direct Grant Access link and the warning icon. Under Changed, "Needs permission" is now "Needs access".
- `docs/troubleshooting/macos-27-3-permission.md`: replace the manual "System Settings → Privacy & Security → …" navigation with Grant Access… or the Access pane, update the symptom wording, and reference the screenshots.
- README: wherever it says the menu tells you where to grant permission, use the new wording.

## Left to the builder

- **Whether Input Monitoring is required.** Find out with the maintainer before building the pane. Switch butterscroll off under Input Monitoring, restart the job, and check whether the tap installs and works.
  - If it is not required, drop it from the docs and messages (`src/main.rs:178`).
  - If it is required, add a second status row using `IOHIDCheckAccess(kIOHIDRequestTypeListenEvent)` with its own deep link (`x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent`, which opened Input Monitoring on 2026-11-26).
- **Whether `tap::request_access()` already adds butterscroll to the Override list**, so the user only switches it on instead of using `+` and Cmd-Shift-G. If it does, the steps and screenshots shrink to match. Check on the clean account while capturing.
- **How to detect macOS 27.3 or later** for the Grant Access link, for example `sysctl kern.osproductversion` or `NSProcessInfo.operatingSystemVersion`.
- The exact look of the warning icon, the Access pane's sidebar icon and its tint.
- **Whether polling `can_create` every second in the Settings process is cheap and harmless.** It creates and immediately destroys an active tap. If measurement shows any input lag, poll less often or only when the window gains focus.
- Tests: the auto-open rule (open once per failure, again after a recovery) and the macOS-version choice of the deep link are plain logic and get unit tests. The FFI and the UI are checked by running the app.

# mfx runs but does nothing (macOS 27.3 permission)

Read this when `com.jeffujioka.mfx` is `running` in launchd but no button remapping or smooth scrolling happens.

## Symptom

- `launchctl print gui/$(id -u)/com.jeffujioka.mfx` shows `state = running`.
- `~/Library/Logs/mfx.log` ends with `permission missing …` and `waiting for permission (menu → Grant Access…)`, and never prints `mfx: running (config: …)`.
- The menu-bar status reads "Needs permission".

## Cause

macOS 27.3 beta 2 split the old Accessibility permission (Apple: "Device Control and Data Access") into two settings:

- **System & App Protection Override**: lets an app control the Mac. mfx needs this to create its event tap, which rewrites and swallows events.
- **Accessibility**: now only for assistive technologies. It has no `+` button and lists only apps with an Accessibility designation.

`AXIsProcessTrusted()` does not reflect the Override grant, so mfx versions before this fix waited forever. mfx now decides by trying to create the tap (`tap::can_create`, `tap::install` in `src/macos/tap.rs`).

Source: [macOS 27.3 Beta 2 release notes, Privacy & Security](https://developer.apple.com/documentation/macos-release-notes/macos-27_3-release-notes).

## Diagnose

1. Log: `tail ~/Library/Logs/mfx.log`. `mfx: running` means the tap exists.
2. Do not trust `target/release/mfx check` from a terminal for the launchd job. It checks the terminal app's permission, not the `mfx` binary's.
3. Confirm the tap from outside. A tap of the `mfx run` pid with event mask `e400000` (other mouse down/up/drag + scroll) and `enabled=true` is mfx's. This Swift snippet lists them:

   ```swift
   import CoreGraphics
   var n: UInt32 = 0
   CGGetEventTapList(0, nil, &n)
   var l = [CGEventTapInformation](repeating: CGEventTapInformation(), count: Int(n))
   CGGetEventTapList(n, &l, &n)
   for t in l.prefix(Int(n)) { print(t.tappingProcess, String(t.eventsOfInterest, radix: 16), t.enabled) }
   ```

4. `log show --last 2m --info --debug --predicate 'process == "tccd" AND eventMessage CONTAINS "mfx"'` shows the permission decisions for mfx.

## Fix

Two grants are needed for the `mfx` binary that launchd starts:

1. **Input Monitoring**: System Settings → Privacy & Security → Input Monitoring. Enable `mfx`.
2. **System & App Protection Override**: System Settings → Privacy & Security → System & App Protection Override.
   1. Click `+`.
   2. Press Cmd-Shift-G, enter the full path of the binary (for example `~/mouse-fix-rs/target/release/mfx`, wherever your checkout is), press Return, then Add.
   3. In the sheet "Choose Apps for 'mfx' to Control", choose **Override System Protections**.
   4. Acknowledge the five review items (each "OK" turns into "Undo"), then click **Allow System & App Protection Override**.
   5. Authenticate with Touch ID or your password.

mfx retries once per second, so it logs `mfx: running` without a restart.

### Pitfalls (observed on macOS 27.3 beta 2)

- Do the `+` flow once. Opening `+`, picking mfx again and cancelling disabled the tap again (`event tap was disabled by macOS (type 0xffffffff)` in the log).
- The sheet's "Choose Apps" path creates a per-app scope. It showed an empty list and did not install the tap for mfx. Use "Override System Protections".
- A bare binary may not appear in the list afterwards. Check the log or the tap listing instead.
- The file picker can open on another display. Look there if the picker seems missing.
- Do not use the old Accessibility pane: it has no `+` and mfx's grant is not there.

## After a rebuild

macOS ties the grant to the binary's code signature. An ad-hoc signature (the default of `./scripts/build.sh`) changes on every build, which drops the grant. Build with a stable certificate:

```sh
MFX_SIGN_ID="<certificate name>" ./scripts/build.sh
launchctl kickstart -k gui/$(id -u)/com.jeffujioka.mfx
```

With an Apple Development certificate, a fresh signature still required one new grant, so the first build with a new identity needs the Fix steps once. That the grant then survives later rebuilds is expected but not verified. Apple Development certificates expire yearly and each renewal breaks the grant (ADR 0001, `docs/adr/0001-self-signed-release-cert.md`).

For `mfx` started from a terminal instead of launchd, macOS checks the terminal app's permission, not mfx's.

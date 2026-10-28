# mfx

A small personal macOS mouse enhancer written in Rust. It covers the core of what
[Mac Mouse Fix](https://github.com/noah-nuebling/mac-mouse-fix) does:

- **Button remapping**: buttons 3 and up (middle, back, forward, extra buttons) can send
  back/forward, Mission Control, App Exposé, Spaces left/right, Show Desktop, or any key
  combination.
- **Smooth scrolling** for notched mouse wheels, with acceleration and optional reversed
  direction. Trackpads and Magic Mouse are left untouched.
- **Click and drag**: hold a button and drag to scroll like two fingers on a trackpad,
  or to fire one action per direction (gestures).

Mac Mouse Fix was used only as a behavior reference; no code was copied from it.

## Build

```sh
./scripts/build.sh          # cargo build --release + ad-hoc code signing
```

macOS ties the Accessibility permission to the binary's code signature. An ad-hoc
signature changes on every build, so you have to grant the permission again after
each rebuild. To keep it across rebuilds, create a code-signing certificate
(Keychain Access → Certificate Assistant → Create a Certificate…, type "Code Signing")
and build with it:

```sh
MFX_SIGN_ID="mfx local" ./scripts/build.sh
```

## Run

```sh
target/release/mfx           # menu-bar app (add --debug to log every handled event)
target/release/mfx check     # validates the config and reports Accessibility access
```

mfx runs as a menu-bar icon (a mouse) with no Dock icon. The menu has:

- **Enabled**: pause or resume all remapping and scrolling.
- **Settings…**: opens the settings window.
- **Open Config File** / **Reload Config**.
- **Grant Accessibility Access…**: shown while the permission is missing. mfx keeps
  retrying, so it starts working as soon as you grant access.
- **Quit mfx**.

mfx needs **Accessibility** access (System Settings → Privacy & Security →
Accessibility). When you run mfx from a terminal, macOS gives the permission to the
terminal app. When launchd starts it, the permission goes to the `mfx` binary itself.

### Start at login

```sh
target/release/mfx launch-agent > ~/Library/LaunchAgents/com.jeffujioka.mfx.plist
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.jeffujioka.mfx.plist
# stop: launchctl bootout gui/$(id -u)/com.jeffujioka.mfx
```

Logs go to `~/Library/Logs/mfx.log`. launchd restarts mfx if it crashes. A failed
start, such as a missing permission, is retried every 10 s.

## Configure

Use **Settings…** in the menu (or `mfx settings`). Changes are checked while you edit,
and Save is disabled while something is invalid. The running app reloads the file
within a second of saving.

You can also edit `~/.config/mfx/config.toml` by hand. Any change to the file is picked
up automatically. If the file is invalid, the menu shows the error and mfx keeps
the previous config. See [`src/default-config.toml`](src/default-config.toml) for every
option. Saving from the settings window rewrites the file without its comments.
Buttons are numbered like Mac Mouse Fix: 3 = middle, 4 = back, 5 = forward.

The named actions send the macOS default keyboard shortcuts (Ctrl+↑ Mission Control,
Ctrl+↓ App Exposé, Ctrl+←/→ Spaces, F11 Show Desktop). They only work while those
shortcuts are enabled under System Settings → Keyboard → Keyboard Shortcuts.

## Design

```
src/engine.rs     pure state machine: Input events → Verdict (pass/swallow/invert) + Effects
src/scroll.rs     wheel animator: per-notch distance × acceleration, exponential ease-out
src/config.rs     TOML file model (ConfigFile), validation into Config
src/keys.rs       "cmd+shift+4" → keycode + CG flags
src/settings.rs   egui settings window (separate process, writes the TOML)
src/macos/app.rs  menu-bar app: winit event loop, tray menu, config watcher
src/macos/tap.rs  CGEventTap, event posting, reload/pause
src/macos/ffi.rs  hand-written CoreGraphics/CoreFoundation FFI
```

All behavior lives in `engine`/`scroll` and is covered by `cargo test` without
needing a mouse. The macOS layer only translates events in both directions. Events
that mfx posts are tagged in `kCGEventSourceUserData` so its own tap lets them through.

Out of scope for now: per-app settings, double-click actions (they need a click
delay), and Mac Mouse Fix's private-API trackpad gesture simulation.

## License

MIT

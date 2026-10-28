# mfx

A small personal macOS mouse enhancer written in Rust. It covers the core of what
Mac Mouse Fix does:

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
target/release/mfx check     # validates the config and reports Accessibility access
target/release/mfx --debug   # runs in the foreground and logs every handled event
```

The first run asks for **Accessibility** access (System Settings → Privacy & Security →
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

```sh
mkdir -p ~/.config/mfx && target/release/mfx default-config > ~/.config/mfx/config.toml
```

See [`src/default-config.toml`](src/default-config.toml) for every option. Restart mfx
after you edit the file. Buttons are numbered like Mac Mouse Fix: 3 = middle,
4 = back, 5 = forward.

The named actions send the macOS default keyboard shortcuts (Ctrl+↑ Mission Control,
Ctrl+↓ App Exposé, Ctrl+←/→ Spaces, F11 Show Desktop). They only work while those
shortcuts are enabled under System Settings → Keyboard → Keyboard Shortcuts.

## Design

```
src/engine.rs   pure state machine: Input events → Verdict (pass/swallow/invert) + Effects
src/scroll.rs   wheel animator: per-notch distance × acceleration, exponential ease-out
src/config.rs   TOML parsing and validation
src/keys.rs     "cmd+shift+4" → keycode + CG flags
src/macos/      hand-written CoreGraphics/CoreFoundation FFI, CGEventTap, event posting
```

All behavior lives in `engine`/`scroll` and is covered by `cargo test` without
needing a mouse. The macOS layer only translates events in both directions. Events
that mfx posts are tagged in `kCGEventSourceUserData` so its own tap lets them through.

Out of scope for now: per-app settings, double-click actions (they need a click
delay), a menu-bar UI, and Mac Mouse Fix's private-API trackpad gesture simulation.

## License

MIT

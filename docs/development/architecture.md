# How butterscroll works

```
src/engine.rs     pure state machine: Input events → Verdict (pass/swallow/invert) + Effects
src/scroll.rs     wheel animator: per-notch distance × acceleration, critically damped spring (eases in, glides out)
src/config.rs     TOML file model (ConfigFile), validation into Config
src/keys.rs       "cmd+shift+4" → keycode + CG flags
src/settings.rs   egui settings window (separate process, writes the TOML)
src/macos/app.rs  menu-bar app: winit event loop, tray menu, config watcher
src/macos/tap.rs  CGEventTap, event posting, reload/pause
src/macos/ffi.rs  hand-written CoreGraphics/CoreFoundation FFI
```

All behavior lives in `engine`/`scroll` and is covered by `cargo test` without
needing a mouse. The macOS layer only translates events in both directions. Events
that butterscroll posts are tagged in `kCGEventSourceUserData` so its own tap lets them through.

Out of scope for now: per-app settings, double-click actions (they need a click
delay), and Mac Mouse Fix's private-API trackpad gesture simulation.

Mac Mouse Fix was used only as a behavior reference; no code was copied from it.

## README art

The README images live in [`docs/assets/readme/`](../assets/readme/):

- `hero.svg`: animated banner (CSS animations, text converted to outlines, honours
  reduced motion).
- `demo.gif`: an animated illustration of the three features, drawn frame by frame. It is
  not a screen recording.
- `showcase.png` and `feat-*.png`: real screenshots of the settings window in dark
  appearance.

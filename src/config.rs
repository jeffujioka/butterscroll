//! TOML configuration: parsing, validation and the built-in default.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::keys::Shortcut;

/// Shipped default; `mfx default-config` prints exactly this text.
pub const DEFAULT_TOML: &str = include_str!("default-config.toml");

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub scroll: ScrollConfig,
    pub drag: DragConfig,
    /// Keyed by CoreGraphics button number (0 = left, 1 = right, 2 = middle, ...).
    pub buttons: BTreeMap<u32, ButtonBinding>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScrollConfig {
    pub smooth: bool,
    pub invert: bool,
    /// Pixels scrolled per wheel notch before acceleration.
    pub step_px: f64,
    /// Time for an animation to cover ~95% of its distance.
    pub duration_ms: f64,
    pub acceleration: bool,
    /// Notches closer together than this count as one fast streak.
    pub accel_window_ms: f64,
    /// Multiplier gained per notch in a streak.
    pub accel_per_notch: f64,
    pub accel_max: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DragConfig {
    /// Movement (px) below which a press still counts as a click.
    pub click_slop_px: f64,
    /// Movement (px) along one axis that triggers a gesture.
    pub gesture_threshold_px: f64,
    pub scroll_speed: f64,
    pub scroll_invert: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ButtonBinding {
    pub click: Action,
    pub drag: DragMode,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DragMode {
    /// Movement is ignored; the press is a click.
    None,
    /// Dragging past the threshold fires one directional action.
    Gestures(Gestures),
    /// Dragging scrolls like two fingers on a trackpad.
    Scroll,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Gestures {
    pub up: Action,
    pub down: Action,
    pub left: Action,
    pub right: Action,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Do nothing.
    None,
    /// Re-post the original button click.
    Passthrough,
    /// Press a key combination.
    Shortcut(Shortcut),
}

#[derive(Debug)]
pub struct ConfigError(pub String);

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ConfigError {}

fn err<T>(msg: impl Into<String>) -> Result<T, ConfigError> {
    Err(ConfigError(msg.into()))
}

// ---- raw serde layer -------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    #[serde(default)]
    scroll: RawScroll,
    #[serde(default)]
    drag: RawDrag,
    #[serde(default)]
    buttons: BTreeMap<String, RawButton>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, default)]
struct RawScroll {
    smooth: bool,
    invert: bool,
    step_px: f64,
    duration_ms: f64,
    acceleration: bool,
    accel_window_ms: f64,
    accel_per_notch: f64,
    accel_max: f64,
}

impl Default for RawScroll {
    fn default() -> Self {
        Self {
            smooth: true,
            invert: false,
            step_px: 60.0,
            duration_ms: 250.0,
            acceleration: true,
            accel_window_ms: 90.0,
            accel_per_notch: 0.3,
            accel_max: 5.0,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, default)]
struct RawDrag {
    click_slop_px: f64,
    gesture_threshold_px: f64,
    scroll_speed: f64,
    scroll_invert: bool,
}

impl Default for RawDrag {
    fn default() -> Self {
        Self { click_slop_px: 4.0, gesture_threshold_px: 40.0, scroll_speed: 1.0, scroll_invert: false }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawButton {
    #[serde(default)]
    click: Option<RawAction>,
    #[serde(default)]
    drag: Option<String>,
    #[serde(default)]
    gestures: Option<RawGestures>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawGestures {
    up: Option<RawAction>,
    down: Option<RawAction>,
    left: Option<RawAction>,
    right: Option<RawAction>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawAction {
    Named(String),
    Shortcut { shortcut: String },
}

// ---- conversion ------------------------------------------------------------

fn parse_action(raw: Option<RawAction>, default: Action, ctx: &str) -> Result<Action, ConfigError> {
    let Some(raw) = raw else { return Ok(default) };
    let parsed = match raw {
        RawAction::Shortcut { shortcut } => Shortcut::parse(&shortcut).map(Action::Shortcut),
        RawAction::Named(name) => named_action(&name),
    };
    parsed.map_err(|e| ConfigError(format!("{ctx}: {e}")))
}

/// Named actions map to macOS default keyboard shortcuts (System Settings →
/// Keyboard → Keyboard Shortcuts). They only work while those shortcuts are enabled.
fn named_action(name: &str) -> Result<Action, String> {
    let combo = match name {
        "none" => return Ok(Action::None),
        "passthrough" => return Ok(Action::Passthrough),
        "back" => "cmd+[",
        "forward" => "cmd+]",
        "mission-control" => "ctrl+up",
        "app-expose" => "ctrl+down",
        "space-left" => "ctrl+left",
        "space-right" => "ctrl+right",
        "show-desktop" => "f11",
        other => {
            return Err(format!(
                "unknown action {other:?} (expected none, passthrough, back, forward, mission-control, \
                 app-expose, space-left, space-right, show-desktop, or {{ shortcut = \"...\" }})"
            ))
        }
    };
    Shortcut::parse(combo).map(Action::Shortcut)
}

fn positive(v: f64, name: &str) -> Result<f64, ConfigError> {
    if v.is_finite() && v > 0.0 {
        Ok(v)
    } else {
        err(format!("{name} must be a positive number, got {v}"))
    }
}

impl Config {
    pub fn parse(text: &str) -> Result<Config, ConfigError> {
        let raw: RawConfig = toml::from_str(text).map_err(|e| ConfigError(e.to_string()))?;

        let s = raw.scroll;
        let scroll = ScrollConfig {
            smooth: s.smooth,
            invert: s.invert,
            step_px: positive(s.step_px, "scroll.step_px")?,
            duration_ms: positive(s.duration_ms, "scroll.duration_ms")?,
            acceleration: s.acceleration,
            accel_window_ms: positive(s.accel_window_ms, "scroll.accel_window_ms")?,
            accel_per_notch: if s.accel_per_notch >= 0.0 { s.accel_per_notch } else {
                return err("scroll.accel_per_notch must be >= 0");
            },
            accel_max: if s.accel_max >= 1.0 { s.accel_max } else {
                return err("scroll.accel_max must be >= 1");
            },
        };

        let d = raw.drag;
        let drag = DragConfig {
            click_slop_px: positive(d.click_slop_px, "drag.click_slop_px")?,
            gesture_threshold_px: positive(d.gesture_threshold_px, "drag.gesture_threshold_px")?,
            scroll_speed: positive(d.scroll_speed, "drag.scroll_speed")?,
            scroll_invert: d.scroll_invert,
        };
        if drag.gesture_threshold_px <= drag.click_slop_px {
            return err("drag.gesture_threshold_px must be larger than drag.click_slop_px");
        }

        let mut buttons = BTreeMap::new();
        for (key, rb) in raw.buttons {
            // Users count buttons from 1 like Mac Mouse Fix (3 = middle); CoreGraphics from 0.
            let human: u32 = key
                .parse()
                .map_err(|_| ConfigError(format!("buttons.{key}: button must be a number")))?;
            if !(3..=32).contains(&human) {
                return err(format!(
                    "buttons.{key}: only buttons 3..=32 can be remapped (1 and 2 are left/right click)"
                ));
            }
            let ctx = format!("buttons.{key}");
            let click = parse_action(rb.click, Action::Passthrough, &format!("{ctx}.click"))?;
            let drag = match (rb.drag.as_deref().unwrap_or("none"), rb.gestures) {
                ("none", None) => DragMode::None,
                ("scroll", None) => DragMode::Scroll,
                ("gestures", Some(g)) => {
                    let gctx = format!("{ctx}.gestures");
                    DragMode::Gestures(Gestures {
                        up: parse_action(g.up, Action::None, &format!("{gctx}.up"))?,
                        down: parse_action(g.down, Action::None, &format!("{gctx}.down"))?,
                        left: parse_action(g.left, Action::None, &format!("{gctx}.left"))?,
                        right: parse_action(g.right, Action::None, &format!("{gctx}.right"))?,
                    })
                }
                ("gestures", None) => return err(format!("{ctx}: drag = \"gestures\" needs a [{ctx}.gestures] table")),
                ("none" | "scroll", Some(_)) => {
                    return err(format!("{ctx}: a gestures table requires drag = \"gestures\""))
                }
                (other, _) => {
                    return err(format!("{ctx}.drag: unknown mode {other:?} (expected none, gestures, scroll)"))
                }
            };
            buttons.insert(human - 1, ButtonBinding { click, drag });
        }

        Ok(Config { scroll, drag, buttons })
    }

    pub fn load(path: &Path) -> Result<Config, ConfigError> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| ConfigError(format!("reading {}: {e}", path.display())))?;
        Config::parse(&text).map_err(|e| ConfigError(format!("{}: {e}", path.display())))
    }

    /// `$XDG_CONFIG_HOME/mfx/config.toml`, falling back to `~/.config/mfx/config.toml`.
    pub fn default_path() -> Option<PathBuf> {
        let base = match std::env::var_os("XDG_CONFIG_HOME") {
            Some(dir) if !dir.is_empty() => PathBuf::from(dir),
            _ => PathBuf::from(std::env::var_os("HOME")?).join(".config"),
        };
        Some(base.join("mfx").join("config.toml"))
    }
}

impl Default for Config {
    fn default() -> Self {
        Config::parse(DEFAULT_TOML).expect("built-in default config is valid")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_binds_middle_back_forward() {
        let cfg = Config::default();
        assert_eq!(cfg.buttons.keys().copied().collect::<Vec<_>>(), vec![2, 3, 4]);
        assert_eq!(cfg.buttons[&2].drag, DragMode::Scroll);
        assert_eq!(cfg.buttons[&3].click, named_action("back").unwrap());
    }

    #[test]
    fn empty_file_means_smooth_scroll_and_no_bindings() {
        let cfg = Config::parse("").unwrap();
        assert!(cfg.scroll.smooth);
        assert!(cfg.buttons.is_empty());
    }

    #[test]
    fn custom_shortcut_and_gestures() {
        let cfg = Config::parse(
            r#"
            [buttons.6]
            click = { shortcut = "cmd+shift+4" }
            drag = "gestures"
            [buttons.6.gestures]
            up = "mission-control"
            "#,
        )
        .unwrap();
        let b = &cfg.buttons[&5];
        assert_eq!(b.click, Action::Shortcut(Shortcut::parse("shift+cmd+4").unwrap()));
        let DragMode::Gestures(g) = &b.drag else { panic!("expected gestures") };
        assert_eq!(g.up, named_action("mission-control").unwrap());
        assert_eq!(g.down, Action::None);
    }

    #[test]
    fn rejects_bad_input_with_location() {
        for (text, needle) in [
            ("[buttons.1]\nclick = \"back\"", "buttons.1"),
            ("[buttons.3]\nclick = \"teleport\"", "buttons.3.click"),
            ("[buttons.3]\ndrag = \"gestures\"", "needs a [buttons.3.gestures]"),
            ("[buttons.3]\nclick = { shortcut = \"cmd+nope\" }", "nope"),
            ("[scroll]\nstep_px = -1", "scroll.step_px"),
            ("[scroll]\nsmoth = true", "smoth"),
            ("[drag]\nclick_slop_px = 50", "gesture_threshold_px"),
        ] {
            let e = Config::parse(text).unwrap_err().to_string();
            assert!(e.contains(needle), "{text:?} -> {e}");
        }
    }
}

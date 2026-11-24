//! TOML configuration: parsing, validation and the built-in default.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

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

// ---- file model ------------------------------------------------------------
//
// Mirrors the TOML 1:1 so the settings window can edit and save it; `Config`
// is the validated form the engine uses.

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigFile {
    #[serde(default)]
    pub scroll: ScrollFile,
    #[serde(default)]
    pub drag: DragFile,
    /// Keyed by user button number as a string ("3" = middle).
    #[serde(default)]
    pub buttons: BTreeMap<String, ButtonFile>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ScrollFile {
    pub smooth: bool,
    pub invert: bool,
    pub step_px: f64,
    pub duration_ms: f64,
    pub acceleration: bool,
    pub accel_window_ms: f64,
    pub accel_per_notch: f64,
    pub accel_max: f64,
}

impl Default for ScrollFile {
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct DragFile {
    pub click_slop_px: f64,
    pub gesture_threshold_px: f64,
    pub scroll_speed: f64,
    pub scroll_invert: bool,
}

impl Default for DragFile {
    fn default() -> Self {
        Self { click_slop_px: 4.0, gesture_threshold_px: 40.0, scroll_speed: 1.0, scroll_invert: false }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ButtonFile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub click: Option<ActionFile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gestures: Option<GesturesFile>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GesturesFile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub up: Option<ActionFile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub down: Option<ActionFile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left: Option<ActionFile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right: Option<ActionFile>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ActionFile {
    Named(String),
    Shortcut { shortcut: String },
}

/// Drag modes as written in the file.
pub const DRAG_MODES: [&str; 3] = ["none", "scroll", "gestures"];

/// Named actions: (file name, label, key combo or `None` for the special ones).
/// Combos are the macOS default keyboard shortcuts (System Settings → Keyboard →
/// Keyboard Shortcuts); they only work while those shortcuts are enabled.
pub const NAMED_ACTIONS: [(&str, &str, Option<&str>); 9] = [
    ("none", "Nothing", None),
    ("passthrough", "Normal click", None),
    ("back", "Back", Some("cmd+[")),
    ("forward", "Forward", Some("cmd+]")),
    ("mission-control", "Mission Control", Some("ctrl+up")),
    ("app-expose", "App Exposé", Some("ctrl+down")),
    ("space-left", "Space left", Some("ctrl+left")),
    ("space-right", "Space right", Some("ctrl+right")),
    ("show-desktop", "Show Desktop", Some("f11")),
];

// ---- conversion ------------------------------------------------------------

fn parse_action(raw: Option<&ActionFile>, default: Action, ctx: &str) -> Result<Action, ConfigError> {
    let Some(raw) = raw else { return Ok(default) };
    let parsed = match raw {
        ActionFile::Shortcut { shortcut } => Shortcut::parse(shortcut).map(Action::Shortcut),
        ActionFile::Named(name) => named_action(name),
    };
    parsed.map_err(|e| ConfigError(format!("{ctx}: {e}")))
}

fn named_action(name: &str) -> Result<Action, String> {
    match NAMED_ACTIONS.iter().find(|(n, _, _)| *n == name) {
        Some((_, _, Some(combo))) => Shortcut::parse(combo).map(Action::Shortcut),
        Some(("passthrough", _, None)) => Ok(Action::Passthrough),
        Some(_) => Ok(Action::None),
        None => {
            let names: Vec<_> = NAMED_ACTIONS.iter().map(|(n, _, _)| *n).collect();
            Err(format!(
                "unknown action {name:?} (expected {}, or {{ shortcut = \"...\" }})",
                names.join(", ")
            ))
        }
    }
}

fn positive(v: f64, name: &str) -> Result<f64, ConfigError> {
    if v.is_finite() && v > 0.0 {
        Ok(v)
    } else {
        err(format!("{name} must be a positive number, got {v}"))
    }
}

/// Highest button number that can be bound (user numbering, 3 = middle).
pub const MAX_BUTTON: u32 = 32;

impl ConfigFile {
    pub fn parse(text: &str) -> Result<ConfigFile, ConfigError> {
        toml::from_str(text).map_err(|e| ConfigError(e.to_string()))
    }

    /// TOML as the settings window saves it (comments of a hand-edited file are not kept).
    pub fn to_toml(&self) -> String {
        let body = toml::to_string_pretty(self).expect("config file model always serializes");
        format!("# Written by `mfx settings`. Options: `mfx default-config`.\n\n{body}")
    }
}

impl Config {
    pub fn parse(text: &str) -> Result<Config, ConfigError> {
        Config::from_file(&ConfigFile::parse(text)?)
    }

    /// Validates the file model.
    pub fn from_file(raw: &ConfigFile) -> Result<Config, ConfigError> {
        let s = &raw.scroll;
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

        let d = &raw.drag;
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
        for (key, rb) in &raw.buttons {
            // Users count buttons from 1 like Mac Mouse Fix (3 = middle); CoreGraphics from 0.
            let human: u32 = key
                .parse()
                .map_err(|_| ConfigError(format!("buttons.{key}: button must be a number")))?;
            if !(3..=MAX_BUTTON).contains(&human) {
                return err(format!(
                    "buttons.{key}: only buttons 3..={MAX_BUTTON} can be remapped (1 and 2 are left/right click)"
                ));
            }
            let ctx = format!("buttons.{key}");
            let click = parse_action(rb.click.as_ref(), Action::Passthrough, &format!("{ctx}.click"))?;
            let drag = match (rb.drag.as_deref().unwrap_or("none"), &rb.gestures) {
                ("none", None) => DragMode::None,
                ("scroll", None) => DragMode::Scroll,
                ("gestures", Some(g)) => {
                    let gctx = format!("{ctx}.gestures");
                    DragMode::Gestures(Gestures {
                        up: parse_action(g.up.as_ref(), Action::None, &format!("{gctx}.up"))?,
                        down: parse_action(g.down.as_ref(), Action::None, &format!("{gctx}.down"))?,
                        left: parse_action(g.left.as_ref(), Action::None, &format!("{gctx}.left"))?,
                        right: parse_action(g.right.as_ref(), Action::None, &format!("{gctx}.right"))?,
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
        assert_eq!(cfg.buttons[&3].click, Action::Passthrough);
        assert_eq!(cfg.buttons[&4].click, Action::Passthrough);
    }

    #[test]
    fn saved_file_reloads_to_the_same_config() {
        let mut file = ConfigFile::parse(DEFAULT_TOML).unwrap();
        file.buttons.insert(
            "7".into(),
            ButtonFile {
                click: Some(ActionFile::Shortcut { shortcut: "cmd+shift+4".into() }),
                ..Default::default()
            },
        );
        let text = file.to_toml();
        assert_eq!(ConfigFile::parse(&text).unwrap(), file);
        assert_eq!(Config::parse(&text).unwrap(), Config::from_file(&file).unwrap());
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

//! Settings window (`mfx settings`): edits the TOML config with live validation.
//! The running menu-bar app notices the saved file and reloads it.
//!
//! Styled after macOS System Settings: a sidebar of panes, grouped cards of
//! label-left / control-right rows, and a footer with Save as the default button.

use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

use eframe::egui::epaint::text::VariationCoords;
use eframe::egui::{
    self, pos2, vec2, Align, Align2, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, FontTweak,
    Frame, Key, Layout, Margin, Modifiers, PointerButton, Pos2, Rect, Response, RichText, Sense, Shape, Stroke,
    StrokeKind, TextStyle, Theme, Ui, UiBuilder, Vec2, ViewportCommand,
};

use crate::config::{
    ActionFile, ButtonFile, Config, ConfigError, ConfigFile, GesturesFile, DEFAULT_TOML, DRAG_MODES, MAX_BUTTON,
    NAMED_ACTIONS,
};
use crate::keys::Shortcut;

/// San Francisco, the macOS system font (a variable font since Catalina).
const SYSTEM_FONT: &str = "/System/Library/Fonts/SFNS.ttf";
const SIDEBAR_WIDTH: f32 = 210.0;
/// Height of the transparent title bar the content runs under (traffic lights).
const TITLEBAR_HEIGHT: f32 = 52.0;
const ROW_HEIGHT: f32 = 36.0;
const ROW_HEIGHT_WITH_HELP: f32 = 48.0;
const CONTROL_WIDTH: f32 = 200.0;
const SLIDER_WIDTH: f32 = 150.0;
const VALUE_WIDTH: f32 = 64.0;

pub fn run(path: PathBuf) -> Result<(), String> {
    let (file, notice) = match std::fs::read_to_string(&path) {
        Ok(text) => match ConfigFile::parse(&text) {
            Ok(file) => (file, None),
            Err(e) => (default_file(), Some(Notice::error(format!("{}: {e}. Showing defaults.", path.display())))),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (default_file(), None),
        Err(e) => return Err(format!("reading {}: {e}", path.display())),
    };
    let app = SettingsApp { path, saved: file.clone(), file, notice, pane: Pane::Scrolling };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("mfx Settings")
            .with_inner_size([760.0, 600.0])
            .with_min_inner_size([680.0, 420.0])
            .with_fullsize_content_view(true)
            .with_titlebar_shown(false)
            .with_title_shown(false)
            .with_active(true),
        ..Default::default()
    };
    eframe::run_native(
        "mfx Settings",
        options,
        Box::new(|cc| {
            install_fonts(&cc.egui_ctx);
            cc.egui_ctx.style_mut_of(Theme::Dark, |s| apply_style(s, &Palette::DARK));
            cc.egui_ctx.style_mut_of(Theme::Light, |s| apply_style(s, &Palette::LIGHT));
            Ok(Box::new(app))
        }),
    )
    .map_err(|e| format!("settings window: {e}"))
}

fn default_file() -> ConfigFile {
    ConfigFile::parse(DEFAULT_TOML).expect("built-in default config is valid")
}

struct Notice {
    text: String,
    error: bool,
}

impl Notice {
    fn error(text: String) -> Self {
        Notice { text, error: true }
    }
    fn info(text: impl Into<String>) -> Self {
        Notice { text: text.into(), error: false }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Pane {
    Scrolling,
    Drag,
    Buttons,
}

impl Pane {
    const ALL: [Pane; 3] = [Pane::Scrolling, Pane::Drag, Pane::Buttons];

    fn title(self) -> &'static str {
        match self {
            Pane::Scrolling => "Scrolling",
            Pane::Drag => "Click and Drag",
            Pane::Buttons => "Buttons",
        }
    }

    /// Icon tile color; none of them is the accent blue so they stay visible when selected.
    fn tint(self) -> Color32 {
        match self {
            Pane::Scrolling => Color32::from_rgb(52, 199, 89),
            Pane::Drag => Color32::from_rgb(175, 82, 222),
            Pane::Buttons => Color32::from_rgb(255, 149, 0),
        }
    }
}

struct SettingsApp {
    path: PathBuf,
    /// Last state on disk, for the dirty check and Revert.
    saved: ConfigFile,
    file: ConfigFile,
    notice: Option<Notice>,
    pane: Pane,
}

impl eframe::App for SettingsApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let p = Palette::of(ui);
        let validation = Config::from_file(&self.file);
        let dirty = self.file != self.saved;
        let can_save = dirty && validation.is_ok();
        if ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::S)) && can_save {
            self.save();
        }

        egui::Panel::left("sidebar")
            .exact_size(SIDEBAR_WIDTH)
            .resizable(false)
            .frame(Frame::new().fill(p.sidebar).inner_margin(Margin { left: 10, right: 10, top: 0, bottom: 14 }))
            .show(ui, |ui| self.sidebar(ui));

        egui::Panel::bottom("footer")
            .frame(Frame::new().fill(p.window).inner_margin(Margin::symmetric(20, 12)))
            .show(ui, |ui| self.footer(ui, validation.as_ref().err(), dirty, can_save));

        egui::Panel::top("header")
            .exact_size(TITLEBAR_HEIGHT)
            .show_separator_line(false)
            .frame(Frame::new().fill(p.window).inner_margin(Margin { left: 24, right: 24, top: 0, bottom: 0 }))
            .show(ui, |ui| {
                move_window_by(ui, ui.max_rect());
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| ui.heading(self.pane.title()));
            });

        egui::CentralPanel::default().frame(Frame::new().fill(p.window)).show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                Frame::new().inner_margin(Margin { left: 24, right: 24, top: 0, bottom: 24 }).show(ui, |ui| {
                    match self.pane {
                        Pane::Scrolling => scroll_pane(ui, &mut self.file),
                        Pane::Drag => drag_pane(ui, &mut self.file),
                        Pane::Buttons => buttons_pane(ui, &mut self.file),
                    }
                });
            });
        });
    }
}

impl SettingsApp {
    fn save(&mut self) {
        self.notice = Some(match save(&self.path, &self.file) {
            Ok(()) => {
                self.saved = self.file.clone();
                Notice::info("Saved. mfx applies it within a second.")
            }
            Err(e) => Notice::error(e),
        });
    }

    fn sidebar(&mut self, ui: &mut Ui) {
        let (strip, _) = ui.allocate_exact_size(vec2(ui.available_width(), TITLEBAR_HEIGHT), Sense::hover());
        move_window_by(ui, strip);
        ui.spacing_mut().item_spacing.y = 2.0;
        for pane in Pane::ALL {
            if sidebar_item(ui, pane, self.pane == pane).clicked() {
                self.pane = pane;
            }
        }

        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            let p = Palette::of(ui);
            let exists = self.path.exists();
            let link = egui::Label::new(RichText::new("Show Config File in Finder").small().color(p.secondary))
                .sense(Sense::click());
            let resp = ui.add_enabled(exists, link).on_hover_text(self.path.display().to_string());
            if resp.clicked() {
                if let Err(e) = std::process::Command::new("open").arg("-R").arg(&self.path).spawn() {
                    self.notice = Some(Notice::error(format!("revealing {}: {e}", self.path.display())));
                }
            }
            if exists {
                resp.on_hover_cursor(egui::CursorIcon::PointingHand);
            }
        });
    }

    fn footer(&mut self, ui: &mut Ui, error: Option<&ConfigError>, dirty: bool, can_save: bool) {
        let p = Palette::of(ui);
        if let Some(e) = error {
            ui.label(RichText::new(format!("⚠  {e}")).color(p.danger));
            ui.add_space(8.0);
        }
        ui.horizontal(|ui| {
            let size = vec2(84.0, 24.0);
            if ui.add(egui::Button::new("Restore Defaults").min_size(size)).clicked() {
                self.file = default_file();
                self.notice = Some(Notice::info("Defaults loaded. Save to apply."));
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let save = if can_save {
                    egui::Button::new(RichText::new("Save").color(Color32::WHITE)).fill(p.accent).stroke(Stroke::NONE)
                } else {
                    egui::Button::new("Save")
                };
                if ui.add_enabled(can_save, save.min_size(size)).on_hover_text("⌘S").clicked() {
                    self.save();
                }
                if ui.add_enabled(dirty, egui::Button::new("Revert").min_size(size)).clicked() {
                    self.file = self.saved.clone();
                    self.notice = None;
                }
                ui.add_space(8.0);
                let status = match &self.notice {
                    Some(n) => Some((n.text.as_str(), if n.error { p.danger } else { p.secondary })),
                    None if dirty => Some(("Unsaved changes", p.secondary)),
                    None => None,
                };
                if let Some((text, color)) = status {
                    ui.add(egui::Label::new(RichText::new(text).color(color)).truncate()).on_hover_text(text);
                }
            });
        });
    }
}

/// Writes via a temp file + rename so the watcher never reads a half-written file.
fn save(path: &Path, file: &ConfigFile) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
    }
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, file.to_toml()).map_err(|e| format!("writing {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("saving {}: {e}", path.display()))
}

// ---- panes -----------------------------------------------------------------

fn scroll_pane(ui: &mut Ui, file: &mut ConfigFile) {
    let s = &mut file.scroll;
    card(ui, |ui| {
        toggle_row(ui, "Smooth scrolling", Some("Animate each notch of the wheel."), &mut s.smooth);
        divider(ui);
        toggle_row(ui, "Reverse direction", Some("Opposite of the system scroll direction."), &mut s.invert);
    });
    footnote(ui, "Applies to notched mouse wheels only. Trackpads and Magic Mouse are never changed.");

    ui.add_enabled_ui(s.smooth, |ui| {
        section(ui, "Animation");
        card(ui, |ui| {
            number_row(ui, "Distance per notch", None, &mut s.step_px, 10.0..=200.0, 1.0, " px");
            divider(ui);
            number_row(ui, "Duration", None, &mut s.duration_ms, 50.0..=800.0, 5.0, " ms");
        });

        section(ui, "Acceleration");
        card(ui, |ui| {
            toggle_row(ui, "Accelerate fast scrolling", Some("Quick consecutive notches scroll further."), &mut s.acceleration);
            ui.add_enabled_ui(s.acceleration, |ui| {
                divider(ui);
                number_row(ui, "Boost per fast notch", None, &mut s.accel_per_notch, 0.0..=1.0, 0.05, "×");
                divider(ui);
                number_row(ui, "Maximum multiplier", None, &mut s.accel_max, 1.0..=10.0, 0.5, "×");
                divider(ui);
                number_row(
                    ui,
                    "Fast-notch window",
                    Some("Notches closer together than this count as fast."),
                    &mut s.accel_window_ms,
                    20.0..=300.0,
                    5.0,
                    " ms",
                );
            });
        });
    });
}

fn drag_pane(ui: &mut Ui, file: &mut ConfigFile) {
    let d = &mut file.drag;
    card(ui, |ui| {
        number_row(
            ui,
            "Click tolerance",
            Some("Moving less than this while holding is still a click."),
            &mut d.click_slop_px,
            1.0..=20.0,
            0.5,
            " px",
        );
        divider(ui);
        number_row(
            ui,
            "Gesture distance",
            Some("Drag this far in one direction to fire a gesture."),
            &mut d.gesture_threshold_px,
            10.0..=200.0,
            1.0,
            " px",
        );
    });

    section(ui, "Drag to Scroll");
    card(ui, |ui| {
        number_row(ui, "Speed", None, &mut d.scroll_speed, 0.1..=5.0, 0.1, "×");
        divider(ui);
        toggle_row(
            ui,
            "Reverse direction",
            Some("Off: content follows your hand, like a trackpad."),
            &mut d.scroll_invert,
        );
    });
    footnote(ui, "Choose which buttons scroll or fire gestures in Buttons.");
}

fn button_name(n: u32) -> String {
    match n {
        3 => "Button 3 (middle)".into(),
        4 => "Button 4 (back)".into(),
        5 => "Button 5 (forward)".into(),
        n => format!("Button {n}"),
    }
}

fn buttons_pane(ui: &mut Ui, file: &mut ConfigFile) {
    let p = Palette::of(ui);
    ui.label(RichText::new("Left and right click are never changed. Buttons not listed here work normally.").weak());

    let mut keys: Vec<String> = file.buttons.keys().cloned().collect();
    keys.sort_by_key(|k| k.parse::<u32>().unwrap_or(u32::MAX));
    let mut remove = None;
    for key in keys {
        let button = file.buttons.get_mut(&key).expect("key taken from the map");
        let title = key.parse().map(button_name).unwrap_or_else(|_| format!("Button {key}"));
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            ui.label(RichText::new(title).font(FontId::new(13.0, semibold())));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let remove_button = egui::Button::new(RichText::new("Remove").color(p.danger)).frame(false);
                if ui.add(remove_button).clicked() {
                    remove = Some(key.clone());
                }
            });
        });
        ui.add_space(2.0);
        card(ui, |ui| button_editor(ui, &key, button));
    }
    if let Some(key) = remove {
        file.buttons.remove(&key);
    }
    if file.buttons.is_empty() {
        ui.add_space(14.0);
        card(ui, |ui| {
            ui.add_sized([ui.available_width(), ROW_HEIGHT], egui::Label::new(RichText::new("No buttons customized.").weak()));
        });
    }

    ui.add_space(14.0);
    let free: Vec<u32> = (3..=MAX_BUTTON).filter(|n| !file.buttons.contains_key(&n.to_string())).collect();
    ui.add_enabled_ui(!free.is_empty(), |ui| {
        ui.menu_button("Add Button…", |ui| {
            ui.set_min_width(170.0);
            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                for n in free {
                    if ui.button(button_name(n)).clicked() {
                        file.buttons.insert(
                            n.to_string(),
                            ButtonFile { click: Some(ActionFile::Named("passthrough".into())), drag: None, gestures: None },
                        );
                        ui.close();
                    }
                }
            });
        });
    });
}

fn button_editor(ui: &mut Ui, key: &str, button: &mut ButtonFile) {
    action_rows(ui, "Click", (key, "click"), &mut button.click, "passthrough");
    divider(ui);

    let current = button.drag.clone().unwrap_or_else(|| "none".into());
    let mut selected = current.clone();
    row(ui, "Drag", Some(help(drag_help(&current))), |ui| {
        popup_button((key, "drag"))
            .selected_text(drag_label(&selected))
            .show_ui(ui, |ui| {
                for mode in DRAG_MODES {
                    ui.selectable_value(&mut selected, mode.to_string(), drag_label(mode));
                }
            });
    });
    if selected != current {
        button.drag = (selected != "none").then(|| selected.clone());
        button.gestures = (selected == "gestures").then(GesturesFile::default);
    }

    if let Some(g) = &mut button.gestures {
        for (dir, slot) in [("Drag up", &mut g.up), ("Drag down", &mut g.down), ("Drag left", &mut g.left), ("Drag right", &mut g.right)] {
            divider(ui);
            action_rows(ui, dir, (key, dir), slot, "none");
        }
    }
}

fn drag_label(mode: &str) -> &'static str {
    match mode {
        "scroll" => "Scroll",
        "gestures" => "Gestures",
        _ => "Nothing",
    }
}

fn drag_help(mode: &str) -> &'static str {
    match mode {
        "scroll" => "Hold and move to scroll, like a trackpad.",
        "gestures" => "Hold and move in a direction to run its action.",
        _ => "Holding and moving still counts as a click.",
    }
}

/// The action combo, plus a shortcut field row when "Key shortcut…" is chosen.
fn action_rows(ui: &mut Ui, label: &str, id: impl egui::AsIdSalt + Copy, slot: &mut Option<ActionFile>, default: &str) {
    row(ui, label, None, |ui| action_combo(ui, id, slot, default));
    if let Some(ActionFile::Shortcut { shortcut }) = slot {
        divider(ui);
        let note = match Shortcut::parse(shortcut) {
            Ok(_) => help("Modifiers cmd, ctrl, alt, shift, fn joined with +."),
            Err(e) => RichText::new(e).small().color(Palette::of(ui).danger),
        };
        row(ui, "Shortcut", Some(note), |ui| {
            ui.add(egui::TextEdit::singleline(shortcut).hint_text("cmd+shift+4").desired_width(CONTROL_WIDTH));
        });
    }
}

/// A combo box drawn like an NSPopUpButton: fixed width, up/down chevrons.
fn popup_button(id: impl egui::AsIdSalt) -> egui::ComboBox {
    egui::ComboBox::from_id_salt(id).width(CONTROL_WIDTH).icon(|ui, rect, visuals, _open| {
        let c = rect.center();
        let stroke = Stroke::new(1.4, visuals.fg_stroke.color);
        for dir in [-1.0, 1.0] {
            let tip = c + vec2(0.0, 5.0 * dir);
            let base = c + vec2(0.0, 1.5 * dir);
            ui.painter().add(Shape::line(vec![base + vec2(-3.0, 0.0), tip, base + vec2(3.0, 0.0)], stroke));
        }
    })
}

/// Combo box over the named actions plus a free-form key shortcut.
fn action_combo(ui: &mut Ui, id: impl egui::AsIdSalt, slot: &mut Option<ActionFile>, default: &str) {
    let label_of = |name: &str| NAMED_ACTIONS.iter().find(|(n, _, _)| *n == name).map_or("Unknown", |(_, l, _)| *l);
    let current_label = match slot {
        None => label_of(default),
        Some(ActionFile::Named(n)) => label_of(n),
        Some(ActionFile::Shortcut { .. }) => "Key shortcut",
    };
    popup_button(id).selected_text(current_label).show_ui(ui, |ui| {
        for (name, label, combo) in NAMED_ACTIONS {
            let selected = matches!(slot, Some(ActionFile::Named(n)) if n == name) || (slot.is_none() && name == default);
            let mut item = egui::Button::selectable(selected, label);
            if let Some(c) = combo {
                item = item.right_text(RichText::new(pretty_combo(c)).weak());
            }
            if ui.add(item).clicked() {
                *slot = Some(ActionFile::Named(name.into()));
            }
        }
        ui.separator();
        let is_custom = matches!(slot, Some(ActionFile::Shortcut { .. }));
        if ui.selectable_label(is_custom, "Key shortcut…").clicked() && !is_custom {
            *slot = Some(ActionFile::Shortcut { shortcut: String::new() });
        }
    });
}

/// `"ctrl+up"` → `"⌃↑"`, the way macOS menus show shortcuts.
fn pretty_combo(combo: &str) -> String {
    combo
        .split('+')
        .map(|k| match k {
            "cmd" | "command" => "⌘".to_string(),
            "ctrl" | "control" => "⌃".to_string(),
            "alt" | "opt" | "option" => "⌥".to_string(),
            "shift" => "⇧".to_string(),
            "up" => "↑".to_string(),
            "down" => "↓".to_string(),
            "left" => "←".to_string(),
            "right" => "→".to_string(),
            k => k.to_uppercase(),
        })
        .collect()
}

// ---- form building blocks ---------------------------------------------------

/// A rounded group of rows, like the inset grouped forms in System Settings.
fn card(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    let p = Palette::of(ui);
    Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(1.0, p.card_stroke))
        .corner_radius(10)
        .inner_margin(Margin::symmetric(14, 0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            add(ui);
        });
}

fn divider(ui: &mut Ui) {
    let p = Palette::of(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    let hairline = 1.0 / ui.ctx().pixels_per_point();
    ui.painter().hline(rect.x_range(), rect.center().y, Stroke::new(hairline, p.separator));
}

fn section(ui: &mut Ui, title: &str) {
    ui.add_space(18.0);
    ui.horizontal(|ui| {
        ui.add_space(4.0);
        ui.label(RichText::new(title).font(FontId::new(13.0, semibold())));
    });
    ui.add_space(4.0);
}

fn footnote(ui: &mut Ui, text: &str) {
    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        ui.add_space(4.0);
        ui.label(help(text));
    });
}

fn help(text: &str) -> RichText {
    RichText::new(text).small().weak()
}

/// One form row: label (and optional help line) on the left, controls right-aligned.
/// Controls are added right to left, so the first one added sits at the right edge.
fn row(ui: &mut Ui, label: &str, help: Option<RichText>, controls: impl FnOnce(&mut Ui)) {
    let height = if help.is_some() { ROW_HEIGHT_WITH_HELP } else { ROW_HEIGHT };
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let mut right = ui.new_child(UiBuilder::new().max_rect(rect).layout(Layout::right_to_left(Align::Center)));
    controls(&mut right);
    let controls_left = right.min_rect().left();

    let line_gap = 1.0;
    let mut text_height = ui.text_style_height(&TextStyle::Body);
    if help.is_some() {
        text_height += line_gap + ui.text_style_height(&TextStyle::Small);
    }
    let top = rect.top() + ((height - text_height) / 2.0).max(0.0);
    let label_rect = Rect::from_min_max(pos2(rect.left(), top), pos2(controls_left - 12.0, rect.bottom()));
    let mut left = ui.new_child(UiBuilder::new().max_rect(label_rect).layout(Layout::top_down(Align::Min)));
    left.spacing_mut().item_spacing.y = line_gap;
    left.add(egui::Label::new(label).truncate());
    if let Some(help) = help {
        left.add(egui::Label::new(help).truncate());
    }
}

fn toggle_row(ui: &mut Ui, label: &str, help_text: Option<&str>, on: &mut bool) {
    row(ui, label, help_text.map(help), |ui| {
        toggle(ui, on);
    });
}

fn number_row(
    ui: &mut Ui,
    label: &str,
    help_text: Option<&str>,
    value: &mut f64,
    range: RangeInclusive<f64>,
    step: f64,
    unit: &str,
) {
    row(ui, label, help_text.map(help), |ui| {
        let field = egui::DragValue::new(value)
            .range(range.clone())
            .clamp_existing_to_range(false)
            .speed(step)
            .max_decimals(2)
            .suffix(unit);
        ui.add_sized([VALUE_WIDTH, 22.0], field);
        slider(ui, value, range, step);
    });
}

/// macOS-style switch.
fn toggle(ui: &mut Ui, on: &mut bool) -> Response {
    let p = Palette::of(ui);
    let (rect, mut resp) = ui.allocate_exact_size(vec2(32.0, 18.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, ui.is_enabled(), *on, ""));
    if ui.is_rect_visible(rect) {
        let t = ui.ctx().animate_bool(resp.id, *on);
        let radius = rect.height() / 2.0;
        ui.painter().rect_filled(rect, radius, p.track.lerp_to_gamma(p.accent, t));
        let x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), t);
        let knob = pos2(x, rect.center().y);
        ui.painter().circle(knob, radius - 2.0, Color32::WHITE, Stroke::new(0.5, Color32::from_black_alpha(50)));
    }
    resp
}

/// macOS-style slider: thin track filled with the accent up to a round knob.
/// Values outside `range` (hand-edited file) are left alone until dragged.
fn slider(ui: &mut Ui, value: &mut f64, range: RangeInclusive<f64>, step: f64) -> Response {
    let p = Palette::of(ui);
    let knob_radius = 8.0;
    let (rect, mut resp) = ui.allocate_exact_size(vec2(SLIDER_WIDTH, 20.0), Sense::click_and_drag());
    let (lo, hi) = (*range.start(), *range.end());
    let track = (rect.left() + knob_radius)..=(rect.right() - knob_radius);
    let snap = |v: f64| (((v / step).round() * step * 1e9).round() / 1e9).clamp(lo, hi);

    let mut target = None;
    if let Some(pos) = resp.interact_pointer_pos() {
        let t = f64::from(egui::remap_clamp(pos.x, track.clone(), 0.0..=1.0));
        target = Some(snap(lo + t * (hi - lo)));
    } else if resp.has_focus() {
        let steps = ui.input(|i| {
            i.num_presses(Key::ArrowRight) as i32 + i.num_presses(Key::ArrowUp) as i32
                - i.num_presses(Key::ArrowLeft) as i32
                - i.num_presses(Key::ArrowDown) as i32
        });
        if steps != 0 {
            target = Some(snap(*value + f64::from(steps) * step));
        }
    }
    if let Some(v) = target.filter(|v| v != value) {
        *value = v;
        resp.mark_changed();
    }
    resp.widget_info(|| egui::WidgetInfo::slider(ui.is_enabled(), *value, ""));

    if ui.is_rect_visible(rect) {
        let t = ((*value - lo) / (hi - lo)).clamp(0.0, 1.0) as f32;
        let knob = pos2(egui::lerp(track, t), rect.center().y);
        let rail = Rect::from_min_max(pos2(rect.left(), knob.y - 2.0), pos2(rect.right(), knob.y + 2.0));
        let painter = ui.painter();
        painter.rect_filled(rail, 2.0, p.track);
        painter.rect_filled(Rect::from_min_max(rail.min, pos2(knob.x, rail.max.y)), 2.0, p.accent);
        painter.circle_filled(knob + vec2(0.0, 0.5), knob_radius + 0.5, Color32::from_black_alpha(40));
        painter.circle(knob, knob_radius, p.knob, Stroke::new(0.5, p.knob_stroke));
    }
    resp
}

// ---- window chrome -----------------------------------------------------------

/// The title bar is transparent and content runs under it, so empty strips at the
/// top have to move the window themselves.
fn move_window_by(ui: &Ui, rect: Rect) {
    let resp = ui.interact(rect, ui.id().with("move-window"), Sense::click_and_drag());
    if resp.drag_started_by(PointerButton::Primary) {
        ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
    }
}

fn sidebar_item(ui: &mut Ui, pane: Pane, selected: bool) -> Response {
    let p = Palette::of(ui);
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, pane.title()));
    let painter = ui.painter();
    let fill = if selected {
        p.accent
    } else if resp.hovered() {
        p.text.gamma_multiply(0.08)
    } else {
        Color32::TRANSPARENT
    };
    painter.rect_filled(rect, 6.0, fill);
    let icon = Rect::from_center_size(pos2(rect.left() + 18.0, rect.center().y), Vec2::splat(20.0));
    paint_icon(painter, icon, pane);
    let color = if selected { Color32::WHITE } else { p.text };
    painter.text(pos2(icon.right() + 8.0, rect.center().y), Align2::LEFT_CENTER, pane.title(), FontId::proportional(13.0), color);
    resp
}

/// Small white glyph on a colored rounded tile, like the System Settings sidebar.
fn paint_icon(painter: &egui::Painter, tile: Rect, pane: Pane) {
    painter.rect_filled(tile, 5.0, pane.tint());
    let stroke = Stroke::new(1.6, Color32::WHITE);
    let c = tile.center();
    let arrow = |tip: Pos2, dir: Vec2| {
        let back = tip - dir * 3.0;
        let side = dir.rot90() * 3.0;
        painter.add(Shape::line(vec![back + side, tip, back - side], stroke));
    };
    match pane {
        Pane::Scrolling => {
            let (top, bottom) = (c - vec2(0.0, 6.0), c + vec2(0.0, 6.0));
            painter.line_segment([top, bottom], stroke);
            arrow(top, -Vec2::Y);
            arrow(bottom, Vec2::Y);
        }
        Pane::Drag => {
            for dir in [Vec2::X, -Vec2::X, Vec2::Y, -Vec2::Y] {
                let tip = c + dir * 6.0;
                painter.line_segment([c, tip], stroke);
                arrow(tip, dir);
            }
        }
        Pane::Buttons => {
            let body = Rect::from_center_size(c, vec2(9.0, 13.0));
            painter.rect_stroke(body, 4.5, stroke, StrokeKind::Middle);
            painter.line_segment([pos2(c.x, body.top()), pos2(c.x, c.y - 1.0)], stroke);
        }
    }
}

// ---- look --------------------------------------------------------------------

/// System colors (macOS 14 appearance), per theme.
#[derive(Clone, Copy)]
struct Palette {
    window: Color32,
    sidebar: Color32,
    card: Color32,
    card_stroke: Color32,
    separator: Color32,
    text: Color32,
    secondary: Color32,
    control: Color32,
    control_hover: Color32,
    control_stroke: Color32,
    track: Color32,
    knob: Color32,
    knob_stroke: Color32,
    accent: Color32,
    danger: Color32,
}

impl Palette {
    const DARK: Palette = Palette {
        window: Color32::from_rgb(30, 30, 30),
        sidebar: Color32::from_rgb(40, 40, 42),
        card: Color32::from_rgb(40, 40, 42),
        card_stroke: Color32::from_rgb(52, 52, 54),
        separator: Color32::from_rgb(64, 64, 66),
        text: Color32::from_rgb(229, 229, 234),
        secondary: Color32::from_rgb(152, 152, 157),
        control: Color32::from_rgb(68, 68, 70),
        control_hover: Color32::from_rgb(82, 82, 84),
        control_stroke: Color32::from_rgb(78, 78, 80),
        track: Color32::from_rgb(72, 72, 74),
        knob: Color32::from_rgb(232, 232, 234),
        knob_stroke: Color32::from_rgb(20, 20, 20),
        accent: Color32::from_rgb(10, 132, 255),
        danger: Color32::from_rgb(255, 69, 58),
    };

    const LIGHT: Palette = Palette {
        window: Color32::from_rgb(242, 242, 245),
        sidebar: Color32::from_rgb(232, 232, 235),
        card: Color32::WHITE,
        card_stroke: Color32::from_rgb(222, 222, 226),
        separator: Color32::from_rgb(226, 226, 230),
        text: Color32::from_rgb(29, 29, 31),
        secondary: Color32::from_rgb(110, 110, 115),
        control: Color32::WHITE,
        control_hover: Color32::from_rgb(246, 246, 248),
        control_stroke: Color32::from_rgb(200, 200, 205),
        track: Color32::from_rgb(222, 222, 226),
        knob: Color32::WHITE,
        knob_stroke: Color32::from_rgb(190, 190, 195),
        accent: Color32::from_rgb(0, 122, 255),
        danger: Color32::from_rgb(255, 59, 48),
    };

    fn of(ui: &Ui) -> Palette {
        if ui.visuals().dark_mode {
            Palette::DARK
        } else {
            Palette::LIGHT
        }
    }
}

fn semibold() -> FontFamily {
    FontFamily::Name("semibold".into())
}

/// SF Pro for body text and a semibold instance of it for headings. Falls back to
/// egui's bundled fonts if the system font can't be read.
fn install_fonts(ctx: &egui::Context) {
    let mut defs = FontDefinitions::default();
    let mut semibold_fonts = defs.families[&FontFamily::Proportional].clone();
    static SYSTEM_FONT_BYTES: LazyLock<Option<Vec<u8>>> = LazyLock::new(|| std::fs::read(SYSTEM_FONT).ok());
    // Both weights borrow this one copy of the font.
    if let Some(bytes) = SYSTEM_FONT_BYTES.as_deref() {
        let weight = |wght: f32| {
            let tweak = FontTweak { coords: VariationCoords::new([("wght", wght)]), ..Default::default() };
            Arc::new(FontData::from_static(bytes).tweak(tweak))
        };
        defs.font_data.insert("sf".into(), weight(400.0));
        defs.font_data.insert("sf-semibold".into(), weight(600.0));
        defs.families.entry(FontFamily::Proportional).or_default().insert(0, "sf".into());
        semibold_fonts.insert(0, "sf-semibold".into());
    }
    defs.families.insert(semibold(), semibold_fonts);
    ctx.set_fonts(defs);
}

fn apply_style(style: &mut egui::Style, p: &Palette) {
    style.text_styles = [
        (TextStyle::Small, FontId::proportional(11.0)),
        (TextStyle::Body, FontId::proportional(13.0)),
        (TextStyle::Button, FontId::proportional(13.0)),
        (TextStyle::Heading, FontId::new(17.0, semibold())),
        (TextStyle::Monospace, FontId::monospace(12.0)),
    ]
    .into();

    let s = &mut style.spacing;
    s.item_spacing = vec2(8.0, 6.0);
    s.button_padding = vec2(10.0, 3.0);
    s.interact_size.y = 22.0;
    s.menu_margin = Margin::same(6);

    let v = &mut style.visuals;
    v.panel_fill = p.window;
    v.window_fill = p.card;
    v.window_stroke = Stroke::new(1.0, p.card_stroke);
    v.window_corner_radius = CornerRadius::same(10);
    v.menu_corner_radius = CornerRadius::same(8);
    v.extreme_bg_color = p.control;
    v.text_edit_bg_color = Some(p.control);
    v.faint_bg_color = p.card;
    v.hyperlink_color = p.accent;
    v.weak_text_color = Some(p.secondary);
    v.selection.bg_fill = p.accent;
    v.selection.stroke = Stroke::new(1.0, Color32::WHITE);

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.card;
    w.noninteractive.weak_bg_fill = p.card;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.separator);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    for (state, fill) in [
        (&mut w.inactive, p.control),
        (&mut w.hovered, p.control_hover),
        (&mut w.active, p.control_hover),
        (&mut w.open, p.control_hover),
    ] {
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.bg_stroke = Stroke::new(1.0, p.control_stroke);
        state.fg_stroke = Stroke::new(1.0, p.text);
        state.corner_radius = CornerRadius::same(6);
        state.expansion = 0.0;
    }
}

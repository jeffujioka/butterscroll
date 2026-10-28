//! Settings window (`mfx settings`): edits the TOML config with live validation.
//! The running menu-bar app notices the saved file and reloads it.

use std::path::{Path, PathBuf};

use eframe::egui::{self, Color32, RichText};

use crate::config::{
    ActionFile, ButtonFile, Config, ConfigFile, GesturesFile, DEFAULT_TOML, DRAG_MODES, MAX_BUTTON, NAMED_ACTIONS,
};
use crate::keys::Shortcut;

pub fn run(path: PathBuf) -> Result<(), String> {
    let (file, notice) = match std::fs::read_to_string(&path) {
        Ok(text) => match ConfigFile::parse(&text) {
            Ok(file) => (file, None),
            Err(e) => (default_file(), Some(Notice::error(format!("{}: {e}. Showing defaults.", path.display())))),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (default_file(), None),
        Err(e) => return Err(format!("reading {}: {e}", path.display())),
    };
    let app = SettingsApp { path, saved: file.clone(), file, notice, new_button: 6 };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("mfx Settings")
            .with_inner_size([560.0, 720.0])
            .with_min_inner_size([460.0, 400.0])
            .with_active(true),
        ..Default::default()
    };
    eframe::run_native("mfx Settings", options, Box::new(|_cc| Ok(Box::new(app))))
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

struct SettingsApp {
    path: PathBuf,
    /// Last state on disk, for the dirty check and Revert.
    saved: ConfigFile,
    file: ConfigFile,
    notice: Option<Notice>,
    new_button: u32,
}

impl eframe::App for SettingsApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let validation = Config::from_file(&self.file);
        let dirty = self.file != self.saved;

        egui::Panel::bottom("actions").show(ui, |ui| {
            ui.add_space(6.0);
            if let Err(e) = &validation {
                ui.colored_label(Color32::from_rgb(220, 70, 60), e.to_string());
            }
            ui.horizontal(|ui| {
                if ui.add_enabled(dirty && validation.is_ok(), egui::Button::new("Save")).clicked() {
                    self.notice = Some(match save(&self.path, &self.file) {
                        Ok(()) => {
                            self.saved = self.file.clone();
                            Notice::info("Saved. mfx applies it within a second.")
                        }
                        Err(e) => Notice::error(e),
                    });
                }
                if ui.add_enabled(dirty, egui::Button::new("Revert")).clicked() {
                    self.file = self.saved.clone();
                    self.notice = None;
                }
                if ui.button("Restore Defaults").clicked() {
                    self.file = default_file();
                    self.notice = Some(Notice::info("Defaults loaded. Save to apply."));
                }
                if let Some(n) = &self.notice {
                    let color = if n.error { Color32::from_rgb(220, 70, 60) } else { ui.visuals().weak_text_color() };
                    ui.colored_label(color, &n.text);
                } else if dirty {
                    ui.weak("Unsaved changes");
                }
            });
            ui.add_space(6.0);
        });

        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.weak(format!("Config file: {}", self.path.display()));
                ui.add_space(8.0);
                scroll_section(ui, &mut self.file);
                ui.add_space(12.0);
                drag_section(ui, &mut self.file);
                ui.add_space(12.0);
                buttons_section(ui, &mut self.file, &mut self.new_button);
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

fn scroll_section(ui: &mut egui::Ui, file: &mut ConfigFile) {
    let s = &mut file.scroll;
    ui.heading("Scrolling");
    ui.weak("Applies to notched mouse wheels only. Trackpads and Magic Mouse are never changed.");
    ui.checkbox(&mut s.smooth, "Smooth scrolling");
    ui.checkbox(&mut s.invert, "Reverse scroll direction");
    ui.add_enabled_ui(s.smooth, |ui| {
        ui.add(egui::Slider::new(&mut s.step_px, 10.0..=200.0).text("Distance per notch (px)"));
        ui.add(egui::Slider::new(&mut s.duration_ms, 50.0..=800.0).text("Animation length (ms)"));
        ui.checkbox(&mut s.acceleration, "Accelerate fast scrolling");
        ui.add_enabled_ui(s.acceleration, |ui| {
            ui.add(egui::Slider::new(&mut s.accel_per_notch, 0.0..=1.0).text("Acceleration per notch"));
            ui.add(egui::Slider::new(&mut s.accel_max, 1.0..=10.0).text("Maximum multiplier"));
            ui.add(egui::Slider::new(&mut s.accel_window_ms, 20.0..=300.0).text("Fast-notch window (ms)"));
        });
    });
}

fn drag_section(ui: &mut egui::Ui, file: &mut ConfigFile) {
    let d = &mut file.drag;
    ui.heading("Click and Drag");
    ui.add(egui::Slider::new(&mut d.click_slop_px, 1.0..=20.0).text("Click tolerance (px)"));
    ui.add(egui::Slider::new(&mut d.gesture_threshold_px, 10.0..=200.0).text("Gesture distance (px)"));
    ui.add(egui::Slider::new(&mut d.scroll_speed, 0.1..=5.0).text("Drag-to-scroll speed"));
    ui.checkbox(&mut d.scroll_invert, "Reverse drag-to-scroll (content moves against the hand)");
}

fn button_name(n: u32) -> String {
    match n {
        3 => "Button 3 (middle)".into(),
        4 => "Button 4 (back)".into(),
        5 => "Button 5 (forward)".into(),
        n => format!("Button {n}"),
    }
}

fn buttons_section(ui: &mut egui::Ui, file: &mut ConfigFile, new_button: &mut u32) {
    ui.heading("Buttons");
    ui.weak("Left and right click are never changed. Unlisted buttons work normally.");

    let mut keys: Vec<String> = file.buttons.keys().cloned().collect();
    keys.sort_by_key(|k| k.parse::<u32>().unwrap_or(u32::MAX));
    let mut remove = None;
    for key in keys {
        let button = file.buttons.get_mut(&key).expect("key taken from the map");
        ui.add_space(4.0);
        ui.group(|ui| {
            ui.horizontal(|ui| {
                let title = key.parse().map(button_name).unwrap_or_else(|_| format!("Button {key}"));
                ui.label(RichText::new(title).strong());
                if ui.small_button("Remove").clicked() {
                    remove = Some(key.clone());
                }
            });
            button_editor(ui, &key, button);
        });
    }
    if let Some(key) = remove {
        file.buttons.remove(&key);
    }

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.add(egui::DragValue::new(new_button).range(3..=MAX_BUTTON).prefix("Button "));
        let exists = file.buttons.contains_key(&new_button.to_string());
        if ui.add_enabled(!exists, egui::Button::new("Add")).clicked() {
            file.buttons.insert(
                new_button.to_string(),
                ButtonFile { click: Some(ActionFile::Named("passthrough".into())), drag: None, gestures: None },
            );
        }
        if exists {
            ui.weak("already configured");
        }
    });
}

fn button_editor(ui: &mut egui::Ui, key: &str, button: &mut ButtonFile) {
    egui::Grid::new(("button", key)).num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
        ui.label("Click");
        action_picker(ui, (key, "click"), &mut button.click, "passthrough");
        ui.end_row();

        ui.label("Drag");
        let current = button.drag.clone().unwrap_or_else(|| "none".into());
        let mut selected = current.clone();
        egui::ComboBox::from_id_salt((key, "drag"))
            .selected_text(drag_label(&selected))
            .show_ui(ui, |ui| {
                for mode in DRAG_MODES {
                    ui.selectable_value(&mut selected, mode.to_string(), drag_label(mode));
                }
            });
        ui.end_row();
        if selected != current {
            button.drag = (selected != "none").then(|| selected.clone());
            button.gestures = (selected == "gestures").then(GesturesFile::default);
        }

        if let Some(g) = &mut button.gestures {
            for (dir, slot) in [("Drag up", &mut g.up), ("Drag down", &mut g.down), ("Drag left", &mut g.left), ("Drag right", &mut g.right)] {
                ui.label(dir);
                action_picker(ui, (key, dir), slot, "none");
                ui.end_row();
            }
        }
    });
}

fn drag_label(mode: &str) -> &'static str {
    match mode {
        "scroll" => "Scroll like a trackpad",
        "gestures" => "Gestures (one action per direction)",
        _ => "Nothing (still a click)",
    }
}

/// Combo box over the named actions plus a free-form key shortcut.
fn action_picker(ui: &mut egui::Ui, id: impl egui::AsIdSalt + Copy, slot: &mut Option<ActionFile>, default: &str) {
    let label_of = |name: &str| NAMED_ACTIONS.iter().find(|(n, _, _)| *n == name).map_or("Unknown", |(_, l, _)| *l);
    let current_label = match slot {
        None => label_of(default).to_string(),
        Some(ActionFile::Named(n)) => label_of(n).to_string(),
        Some(ActionFile::Shortcut { .. }) => "Key shortcut".into(),
    };
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt(id).selected_text(current_label).show_ui(ui, |ui| {
            for (name, label, combo) in NAMED_ACTIONS {
                let selected = matches!(slot, Some(ActionFile::Named(n)) if n == name)
                    || (slot.is_none() && name == default);
                let text = match combo {
                    Some(c) => format!("{label}   ({c})"),
                    None => label.to_string(),
                };
                if ui.selectable_label(selected, text).clicked() {
                    *slot = Some(ActionFile::Named(name.into()));
                }
            }
            ui.separator();
            let is_custom = matches!(slot, Some(ActionFile::Shortcut { .. }));
            if ui.selectable_label(is_custom, "Key shortcut…").clicked() && !is_custom {
                *slot = Some(ActionFile::Shortcut { shortcut: String::new() });
            }
        });
        if let Some(ActionFile::Shortcut { shortcut }) = slot {
            ui.add(egui::TextEdit::singleline(shortcut).hint_text("cmd+shift+4").desired_width(140.0));
            if let Err(e) = Shortcut::parse(shortcut) {
                ui.colored_label(Color32::from_rgb(220, 70, 60), "!").on_hover_text(e);
            }
        }
    });
}

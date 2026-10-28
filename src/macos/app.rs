//! Menu-bar app: owns the main event loop, the status-bar icon and its menu,
//! installs the event tap, and reloads the config when the file changes.

use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};
use winit::application::ApplicationHandler;
use winit::event::{StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
use winit::window::WindowId;

use super::tap;
use crate::config::{Config, DEFAULT_TOML};

const ACCESSIBILITY_PANE: &str = "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility";

enum UserEvent {
    Menu(MenuEvent),
    /// Once per second: config mtime check and tap retry.
    Tick,
}

struct Items {
    status: MenuItem,
    enabled: CheckMenuItem,
    accessibility: MenuItem,
    settings: MenuItem,
    open_config: MenuItem,
    reload: MenuItem,
    quit: MenuItem,
}

struct App {
    config_path: PathBuf,
    debug: bool,
    tray: Option<TrayIcon>,
    items: Option<Items>,
    tap_installed: bool,
    config_mtime: Option<SystemTime>,
    config_error: Option<String>,
    settings_child: Option<Child>,
}

pub fn run(config_path: PathBuf, debug: bool) -> Result<(), String> {
    let event_loop = EventLoop::<UserEvent>::with_user_event()
        // No Dock icon, no app menu: a menu-bar-only agent.
        .with_activation_policy(ActivationPolicy::Accessory)
        .build()
        .map_err(|e| format!("starting the event loop: {e}"))?;
    event_loop.set_control_flow(ControlFlow::Wait);

    let proxy = Mutex::new(event_loop.create_proxy());
    MenuEvent::set_event_handler(Some(move |e| {
        let _ = proxy.lock().map(|p| p.send_event(UserEvent::Menu(e)));
    }));
    let ticker = event_loop.create_proxy();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(1));
        if ticker.send_event(UserEvent::Tick).is_err() {
            break;
        }
    });

    let mut app = App {
        config_path,
        debug,
        tray: None,
        items: None,
        tap_installed: false,
        config_mtime: None,
        config_error: None,
        settings_child: None,
    };
    event_loop.run_app(&mut app).map_err(|e| format!("event loop: {e}"))
}

impl App {
    fn mtime(&self) -> Option<SystemTime> {
        std::fs::metadata(&self.config_path).and_then(|m| m.modified()).ok()
    }

    /// Reads the config file (defaults if absent). On error keeps the running config.
    fn load_config(&mut self) -> Option<Config> {
        self.config_mtime = self.mtime();
        let result = if self.config_path.exists() {
            Config::load(&self.config_path).map_err(|e| e.to_string())
        } else {
            Ok(Config::default())
        };
        match result {
            Ok(cfg) => {
                self.config_error = None;
                Some(cfg)
            }
            Err(e) => {
                eprintln!("mfx: {e}; keeping the previous config");
                self.config_error = Some(e);
                None
            }
        }
    }

    fn try_install_tap(&mut self) {
        if self.tap_installed || !tap::accessibility_trusted(false) {
            return;
        }
        let cfg = self.load_config().unwrap_or_default();
        match tap::install(cfg, self.debug) {
            Ok(()) => {
                self.tap_installed = true;
                eprintln!("mfx: running (config: {})", self.config_path.display());
            }
            Err(e) => eprintln!("mfx: {e}"),
        }
    }

    fn reload(&mut self) {
        if let Some(cfg) = self.load_config() {
            if self.tap_installed {
                tap::reload(cfg);
                eprintln!("mfx: config reloaded");
            }
        }
    }

    fn refresh_menu(&self) {
        let Some(items) = &self.items else { return };
        let status = if !self.tap_installed {
            "Needs Accessibility access".to_string()
        } else if let Some(e) = &self.config_error {
            format!("Config error: {}", truncate(e, 60))
        } else if items.enabled.is_checked() {
            "mfx is running".into()
        } else {
            "mfx is paused".into()
        };
        items.status.set_text(status);
        items.accessibility.set_enabled(!self.tap_installed);
        items.enabled.set_enabled(self.tap_installed);
    }

    fn open_settings(&mut self) {
        if let Some(child) = &mut self.settings_child {
            if matches!(child.try_wait(), Ok(None)) {
                return; // already open
            }
        }
        let spawned = std::env::current_exe().and_then(|exe| {
            Command::new(exe).arg("settings").arg("--config").arg(&self.config_path).spawn()
        });
        match spawned {
            Ok(child) => self.settings_child = Some(child),
            Err(e) => eprintln!("mfx: could not open settings: {e}"),
        }
    }

    fn open_config_file(&self) {
        if !self.config_path.exists() {
            let written = self
                .config_path
                .parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|()| std::fs::write(&self.config_path, DEFAULT_TOML));
            if let Err(e) = written {
                eprintln!("mfx: could not create {}: {e}", self.config_path.display());
                return;
            }
        }
        open(&["-t".as_ref(), self.config_path.as_os_str()]);
    }

    fn handle_menu(&mut self, event: MenuEvent, el: &ActiveEventLoop) {
        let Some(items) = &self.items else { return };
        let id = event.id;
        if id == *items.enabled.id() {
            tap::set_enabled(items.enabled.is_checked());
        } else if id == *items.accessibility.id() {
            tap::accessibility_trusted(true);
            open(&[ACCESSIBILITY_PANE.as_ref()]);
        } else if id == *items.settings.id() {
            self.open_settings();
        } else if id == *items.open_config.id() {
            self.open_config_file();
        } else if id == *items.reload.id() {
            self.reload();
        } else if id == *items.quit.id() {
            el.exit();
        }
        self.refresh_menu();
    }

    fn build_tray(&mut self) {
        let items = Items {
            status: MenuItem::new("mfx", false, None),
            enabled: CheckMenuItem::new("Enabled", true, true, None),
            accessibility: MenuItem::new("Grant Accessibility Access…", true, None),
            settings: MenuItem::new("Settings…", true, None),
            open_config: MenuItem::new("Open Config File", true, None),
            reload: MenuItem::new("Reload Config", true, None),
            quit: MenuItem::new("Quit mfx", true, None),
        };
        let menu = Menu::new();
        let built = menu.append_items(&[
            &items.status,
            &items.enabled,
            &items.accessibility,
            &PredefinedMenuItem::separator(),
            &items.settings,
            &items.open_config,
            &items.reload,
            &PredefinedMenuItem::separator(),
            &items.quit,
        ]);
        if let Err(e) = built {
            eprintln!("mfx: building the menu: {e}");
        }
        let icon = Icon::from_rgba(mouse_icon_rgba(ICON_PX), ICON_PX, ICON_PX).expect("icon size matches buffer");
        match TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_icon_templated(icon)
            .with_tooltip("mfx")
            .build()
        {
            Ok(tray) => self.tray = Some(tray),
            Err(e) => eprintln!("mfx: creating the menu-bar icon: {e}"),
        }
        self.items = Some(items);
    }
}

impl ApplicationHandler<UserEvent> for App {
    fn new_events(&mut self, _el: &ActiveEventLoop, cause: StartCause) {
        // The status item must be created once the app has finished launching.
        if cause == StartCause::Init {
            self.build_tray();
            tap::accessibility_trusted(true); // prompts once if missing
            self.try_install_tap();
            if !self.tap_installed {
                eprintln!("mfx: waiting for Accessibility access (menu → Grant Accessibility Access…)");
            }
            self.refresh_menu();
        }
    }

    fn resumed(&mut self, _el: &ActiveEventLoop) {}

    fn window_event(&mut self, _el: &ActiveEventLoop, _id: WindowId, _event: WindowEvent) {}

    fn user_event(&mut self, el: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Menu(e) => self.handle_menu(e, el),
            UserEvent::Tick => {
                if !self.tap_installed {
                    self.try_install_tap();
                    if self.tap_installed {
                        self.refresh_menu();
                    }
                } else if self.mtime() != self.config_mtime {
                    self.reload();
                    self.refresh_menu();
                }
            }
        }
    }
}

fn open(args: &[&std::ffi::OsStr]) {
    if let Err(e) = Command::new("/usr/bin/open").args(args).spawn() {
        eprintln!("mfx: running open: {e}");
    }
}

fn truncate(s: &str, max: usize) -> String {
    let first_line = s.lines().next().unwrap_or_default();
    if first_line.chars().count() <= max {
        first_line.to_string()
    } else {
        format!("{}…", first_line.chars().take(max).collect::<String>())
    }
}

/// 18 pt at 2x.
const ICON_PX: u32 = 36;

/// Template image of a mouse: outlined capsule body with a button divider.
/// Only alpha matters; macOS tints template images for light/dark menu bars.
fn mouse_icon_rgba(size: u32) -> Vec<u8> {
    let s = size as f32;
    let (cx, cy) = (s / 2.0, s / 2.0);
    let (half_w, half_h, radius) = (s * 0.28, s * 0.44, s * 0.28);
    let stroke = s * 0.075;
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let (px, py) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
            // Signed distance to a rounded rectangle.
            let qx = px.abs() - (half_w - radius);
            let qy = py.abs() - (half_h - radius);
            let outside = qx.max(0.0).hypot(qy.max(0.0));
            let dist = outside + qx.max(qy).min(0.0) - radius;
            let outline = 1.0 - (dist.abs() - stroke / 2.0).clamp(0.0, 1.0);
            // Vertical divider between the two buttons, upper part only.
            let divider = if py < -half_h * 0.15 { 1.0 - (px.abs() - stroke / 2.0).clamp(0.0, 1.0) } else { 0.0 };
            // Horizontal line closing the button area.
            let inside = dist < 0.0;
            let line_y = -half_h * 0.15;
            let bar = if inside { 1.0 - ((py - line_y).abs() - stroke / 2.0).clamp(0.0, 1.0) } else { 0.0 };
            let a = outline.max(divider * inside as u8 as f32).max(bar);
            let i = ((y * size + x) * 4) as usize;
            rgba[i + 3] = (a * 255.0).round() as u8;
        }
    }
    rgba
}

//! butterscroll — a small personal macOS mouse enhancer inspired by Mac Mouse Fix.

mod config;
mod engine;
mod keys;
#[cfg(target_os = "macos")]
mod macos;
mod scroll;
mod settings;
mod update;
mod version;
mod paths;

use std::path::PathBuf;
use std::process::ExitCode;

use paths::BUNDLE_ID;
use config::Config;

const USAGE: &str = "\
butterscroll — mouse button remapping, smooth scrolling and click-and-drag gestures for macOS

USAGE:
    butterscroll [run] [--config PATH] [--debug]   start the menu-bar app (default command)
    butterscroll settings [--config PATH] [--pane access]  open the settings window
    butterscroll check [--config PATH]             validate config and report whether macOS allows the event tap
    butterscroll default-config                    print the default config
    butterscroll launch-agent                      print a LaunchAgent plist that starts butterscroll at login
    butterscroll --version                         print the version

Config: $XDG_CONFIG_HOME/butterscroll/config.toml or ~/.config/butterscroll/config.toml (defaults if missing).
";

struct Args {
    command: String,
    config: Option<PathBuf>,
    debug: bool,
    pane: Option<String>,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args { command: "run".into(), config: None, debug: false, pane: None };
    let mut it = std::env::args().skip(1);
    let mut command_seen = false;
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--config" | "-c" => {
                args.config = Some(it.next().ok_or("--config needs a path")?.into());
            }
            "--pane" => args.pane = Some(it.next().ok_or("--pane needs a value")?),
            "--debug" | "-d" => args.debug = true,
            "-V" | "--version" => args.command = "version".into(),
            "-h" | "--help" => args.command = "help".into(),
            cmd if !command_seen && !cmd.starts_with('-') => {
                args.command = cmd.into();
                command_seen = true;
            }
            other => return Err(format!("unexpected argument {other:?}")),
        }
    }
    Ok(args)
}

fn load_config(explicit: Option<PathBuf>) -> Result<(Config, String), String> {
    if let Some(path) = explicit {
        let cfg = Config::load(&path).map_err(|e| e.to_string())?;
        return Ok((cfg, path.display().to_string()));
    }
    match Config::default_path() {
        Some(path) if path.exists() => {
            let cfg = Config::load(&path).map_err(|e| e.to_string())?;
            Ok((cfg, path.display().to_string()))
        }
        Some(path) => Ok((Config::default(), format!("built-in defaults ({} not found)", path.display()))),
        None => Ok((Config::default(), "built-in defaults ($HOME not set)".into())),
    }
}

fn launch_agent_plist() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| format!("locating butterscroll binary: {e}"))?;
    let home = std::env::var("HOME").map_err(|_| "HOME not set")?;
    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{BUNDLE_ID}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{exe}</string>
        <string>run</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <dict>
        <key>SuccessfulExit</key>
        <false/>
    </dict>
    <key>ThrottleInterval</key>
    <integer>10</integer>
    <key>ProcessType</key>
    <string>Interactive</string>
    <key>StandardErrorPath</key>
    <string>{home}/Library/Logs/butterscroll.log</string>
</dict>
</plist>
"#,
        exe = exe.display()
    ))
}

fn main() -> ExitCode {
    #[cfg(target_os = "macos")]
    if let Err(e) = macos::startup::reexec_if_symlinked() {
        eprintln!("butterscroll: {e}");
    }
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("butterscroll: {e}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match real_main(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("butterscroll: {e}");
            ExitCode::FAILURE
        }
    }
}

fn real_main(args: Args) -> Result<(), String> {
    match args.command.as_str() {
        "version" => {
            println!("butterscroll {}", version::CURRENT);
            Ok(())
        }
        "help" => {
            print!("{USAGE}");
            Ok(())
        }
        "default-config" => {
            print!("{}", config::DEFAULT_TOML);
            Ok(())
        }
        "launch-agent" => {
            print!("{}", launch_agent_plist()?);
            Ok(())
        }
        "check" => {
            println!("binary: {}", std::env::current_exe().map_err(|e| format!("locating butterscroll binary: {e}"))?.display());
            let (cfg, source) = load_config(args.config)?;
            println!("config: OK ({source}), {} button binding(s)", cfg.buttons.len());
            check_permissions()
        }
        "run" => run(config_path(args.config)?, args.debug),
        "settings" => settings::run(config_path(args.config)?, args.pane.as_deref()),
        other => Err(format!("unknown command {other:?}\n\n{USAGE}")),
    }
}

#[cfg(target_os = "macos")]
fn check_permissions() -> Result<(), String> {
    if macos::tap::can_create() {
        println!("event tap: allowed");
        Ok(())
    } else {
        Err(format!("event tap: NOT allowed. {}", permission_hint()))
    }
}

/// macOS grants the permission to the *responsible* app: the terminal when butterscroll is
/// started from a shell, the butterscroll binary itself only when launchd starts it.
#[cfg(target_os = "macos")]
fn permission_hint() -> String {
    const PANE: &str = "System Settings → Privacy & Security → System & App Protection Override \
                        (Accessibility before macOS 27.3) and → Input Monitoring";
    match std::env::var("TERM_PROGRAM") {
        Ok(term) if !term.is_empty() => format!(
            "butterscroll was started from a terminal ({term}), so macOS checks the terminal's permission, not butterscroll's. \
             Either enable {term} under {PANE} and restart it, or start butterscroll through launchd \
             (docs/guide/install.md → \"Start at login\") so the butterscroll entry applies."
        ),
        _ => format!(
            "Enable butterscroll under {PANE}. After a rebuild with an ad-hoc signature, remove and re-add it."
        ),
    }
}

#[cfg(target_os = "macos")]
fn run(config_path: PathBuf, debug: bool) -> Result<(), String> {
    // Held until exit: the lock is what makes a second `butterscroll run` see us as running.
    let _instance = match paths::main_lock().map(|p| macos::instance::try_acquire(&p)) {
        Some(Ok(Some(lock))) => Some(lock),
        Some(Ok(None)) => {
            eprintln!("butterscroll: already running; opening Settings");
            return open_settings(&config_path);
        }
        Some(Err(e)) => {
            eprintln!("butterscroll: single-instance lock: {e}");
            None
        }
        None => None,
    };
    redirect_log_to_file();
    if !macos::tap::can_create() {
        eprintln!("butterscroll: permission missing. {}", permission_hint());
    }
    macos::app::run(config_path, debug)
}

/// Opens the Settings window as a child process, pointed at the same config.
#[cfg(target_os = "macos")]
fn open_settings(config_path: &PathBuf) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("locating butterscroll binary: {e}"))?;
    std::process::Command::new(exe)
        .arg("settings")
        .arg("--config")
        .arg(config_path)
        .spawn()
        .map(drop)
        .map_err(|e| format!("opening Settings: {e}"))
}

/// Rotates the previous log and sends stderr to the log file, unless stderr is a terminal.
/// Failures are printed and ignored: logging must never stop the app.
#[cfg(target_os = "macos")]
fn redirect_log_to_file() {
    use std::io::IsTerminal;
    use macos::startup::{redirect_stderr, rotate_if_larger, LOG_MAX_BYTES};
    if std::io::stderr().is_terminal() {
        return;
    }
    let (Some(log), Some(old)) = (paths::log_file(), paths::old_log_file()) else {
        eprintln!("butterscroll: $HOME not set, not logging to a file");
        return;
    };
    if let Err(e) = rotate_if_larger(&log, &old, LOG_MAX_BYTES) {
        eprintln!("butterscroll: rotating {}: {e}", log.display());
    }
    if let Err(e) = redirect_stderr(&log) {
        eprintln!("butterscroll: logging to {}: {e}", log.display());
    }
}

#[cfg(not(target_os = "macos"))]
fn check_permissions() -> Result<(), String> {
    Err("butterscroll only runs on macOS".into())
}

#[cfg(not(target_os = "macos"))]
fn run(_config_path: PathBuf, _debug: bool) -> Result<(), String> {
    Err("butterscroll only runs on macOS".into())
}

/// The file the app watches and the settings window writes.
fn config_path(explicit: Option<PathBuf>) -> Result<PathBuf, String> {
    explicit.or_else(Config::default_path).ok_or_else(|| "cannot locate the config: $HOME is not set".into())
}

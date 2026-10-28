//! mfx — a small personal macOS mouse enhancer inspired by Mac Mouse Fix.

mod config;
mod engine;
mod keys;
#[cfg(target_os = "macos")]
mod macos;
mod scroll;
mod settings;
mod version;
mod paths;

use std::path::PathBuf;
use std::process::ExitCode;

use paths::BUNDLE_ID;
use config::Config;

const USAGE: &str = "\
mfx — mouse button remapping, smooth scrolling and click-and-drag gestures for macOS

USAGE:
    mfx [run] [--config PATH] [--debug]   start the menu-bar app (default command)
    mfx settings [--config PATH]          open the settings window
    mfx check [--config PATH]             validate config and report Accessibility access
    mfx default-config                    print the default config
    mfx launch-agent                      print a LaunchAgent plist that starts mfx at login
    mfx --version                         print the version

Config: $XDG_CONFIG_HOME/mfx/config.toml or ~/.config/mfx/config.toml (defaults if missing).
";

struct Args {
    command: String,
    config: Option<PathBuf>,
    debug: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args { command: "run".into(), config: None, debug: false };
    let mut it = std::env::args().skip(1);
    let mut command_seen = false;
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--config" | "-c" => {
                args.config = Some(it.next().ok_or("--config needs a path")?.into());
            }
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
    let exe = std::env::current_exe().map_err(|e| format!("locating mfx binary: {e}"))?;
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
    <string>{home}/Library/Logs/mfx.log</string>
</dict>
</plist>
"#,
        exe = exe.display()
    ))
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("mfx: {e}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match real_main(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("mfx: {e}");
            ExitCode::FAILURE
        }
    }
}

fn real_main(args: Args) -> Result<(), String> {
    match args.command.as_str() {
        "version" => {
            println!("mfx {}", version::CURRENT);
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
            let (cfg, source) = load_config(args.config)?;
            println!("config: OK ({source}), {} button binding(s)", cfg.buttons.len());
            check_permissions()
        }
        "run" => run(config_path(args.config)?, args.debug),
        "settings" => settings::run(config_path(args.config)?),
        other => Err(format!("unknown command {other:?}\n\n{USAGE}")),
    }
}

#[cfg(target_os = "macos")]
fn check_permissions() -> Result<(), String> {
    if macos::tap::accessibility_trusted(false) {
        println!("accessibility: granted");
        Ok(())
    } else {
        Err(format!("accessibility: NOT granted. {}", permission_hint()))
    }
}

/// macOS grants Accessibility to the *responsible* app: the terminal when mfx is
/// started from a shell, the mfx binary itself only when launchd starts it.
#[cfg(target_os = "macos")]
fn permission_hint() -> String {
    const PANE: &str = "System Settings → Privacy & Security → Accessibility";
    match std::env::var("TERM_PROGRAM") {
        Ok(term) if !term.is_empty() => format!(
            "mfx was started from a terminal ({term}), so macOS checks the terminal's permission, not mfx's. \
             Either enable {term} under {PANE} and restart it, or start mfx through launchd (README → \
             \"Start at login\") so the mfx entry applies."
        ),
        _ => format!(
            "Enable mfx under {PANE}. After a rebuild with an ad-hoc signature, remove and re-add it."
        ),
    }
}

#[cfg(target_os = "macos")]
fn run(config_path: PathBuf, debug: bool) -> Result<(), String> {
    if !macos::tap::accessibility_trusted(false) {
        eprintln!("mfx: Accessibility access missing. {}", permission_hint());
    }
    macos::app::run(config_path, debug)
}

#[cfg(not(target_os = "macos"))]
fn check_permissions() -> Result<(), String> {
    Err("mfx only runs on macOS".into())
}

#[cfg(not(target_os = "macos"))]
fn run(_config_path: PathBuf, _debug: bool) -> Result<(), String> {
    Err("mfx only runs on macOS".into())
}

/// The file the app watches and the settings window writes.
fn config_path(explicit: Option<PathBuf>) -> Result<PathBuf, String> {
    explicit.or_else(Config::default_path).ok_or_else(|| "cannot locate the config: $HOME is not set".into())
}

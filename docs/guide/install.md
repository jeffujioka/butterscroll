# Install and run butterscroll

butterscroll has no published Release yet, so you build it from source. You need the Rust
toolchain ([rustup](https://rustup.rs)); `rust-toolchain.toml` pins the version and both Mac
architectures (Apple silicon and Intel).

## Build

```sh
git clone https://github.com/jeffujioka/butterscroll.git
cd butterscroll
./scripts/build.sh          # cargo build --release + ad-hoc code signing
```

macOS ties the Accessibility permission to the binary's code signature. An ad-hoc
signature changes on every build, so you have to grant the permission again after
each rebuild. To keep it across rebuilds, create a code-signing certificate
(Keychain Access → Certificate Assistant → Create a Certificate…, type "Code Signing")
and build with it:

```sh
BUTTERSCROLL_SIGN_ID="butterscroll local" ./scripts/build.sh
```

## Run

```sh
target/release/butterscroll           # menu-bar app (add --debug to log every handled event)
target/release/butterscroll check     # validates the config and reports whether macOS allows the event tap
```

butterscroll runs as a menu-bar icon (a mouse) with no Dock icon. The menu has:

- **Enabled**: pause or resume all remapping and scrolling.
- **Settings…**: opens the settings window.
- **Open Config File** / **Reload Config**.
- **Grant Access…**: shown while the permission is missing. butterscroll keeps retrying, so it
  starts working as soon as you grant access.
- **Quit butterscroll**.

Run `butterscroll --help` for every command.

## Permissions

butterscroll needs **Input Monitoring** and permission to control the Mac:

- macOS 27.3 and later: **System & App Protection Override** (System Settings → Privacy &
  Security). The Accessibility pane there is only for assistive technologies and has no
  `+`. In System & App Protection Override click `+`, press Cmd-Shift-G, enter the path
  of the `butterscroll` binary, choose it and press Done. butterscroll starts without a restart.
- Earlier macOS: **Accessibility** (Privacy & Security → Accessibility).

When you run butterscroll from a terminal, macOS gives the permission to the terminal app. When
launchd starts it, the permission goes to the `butterscroll` binary itself.

butterscroll running but doing nothing? See
[butterscroll runs but does nothing](../troubleshooting/macos-27-3-permission.md).

## Start at login

```sh
target/release/butterscroll launch-agent > ~/Library/LaunchAgents/com.jeffujioka.butterscroll.plist
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.jeffujioka.butterscroll.plist
# stop: launchctl bootout gui/$(id -u)/com.jeffujioka.butterscroll
```

Logs go to `~/Library/Logs/butterscroll.log`. launchd restarts butterscroll if it crashes. A failed
start, such as a missing permission, is retried every 10 s.

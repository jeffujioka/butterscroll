# Start at login uses the main-app login item, without crash relaunch

Start at login is implemented with `SMAppService.mainApp` (macOS 13+). The main app is launched only at login; unlike the earlier hand-written LaunchAgent (`KeepAlive`), macOS does not restart mfx if it crashes, and there is no launchd log redirect, so mfx writes its own log to `~/Library/Logs/mfx.log`.

## Considered Options

- **Bundled LaunchAgent via `SMAppService.agent(plistName:)`** (would keep crash relaunch): `unregister()` kills a running agent, so switching Start at login off in Settings would quit mfx itself. The agent would have to be the only running copy, so First launch would need to hand over from the copy the user opened to the agent, and every launch from Finder would need the same hand-over.
- **Hand-written LaunchAgent plist** (the old `mfx launch-agent`): macOS can't report its state through the Login Items API, so the Settings toggle could not reflect it.

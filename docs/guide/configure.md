# Configure butterscroll

## Settings window

Use **Settings…** in the menu (or `butterscroll settings`). The window follows the system
light/dark appearance and has four panes: Scrolling, Click and Drag, Buttons, and Access
(whether macOS lets butterscroll work, and how to allow it; see
[Access](install.md#access)).
Changes are checked while you edit and saved automatically as soon as they are
valid; there is no Save button. The running app reloads the file within a second.

Every setting has a description under it and labels at both ends of its slider. The ⓘ
button next to the scroll timing and acceleration settings works through an example with
your current values. The Try It list at the end of Scrolling lets you feel a change right
after you make it.

## Config file

You can also edit `~/.config/butterscroll/config.toml` (or
`$XDG_CONFIG_HOME/butterscroll/config.toml`) by hand. Any change to the file is picked up
automatically. If the file is invalid, the menu shows the error and butterscroll keeps the
previous config. See [`src/default-config.toml`](../../src/default-config.toml) for every
option, or print it with `butterscroll default-config`. Saving from the settings window
rewrites the file without its comments.

Buttons are numbered from 1: 3 = middle, 4 = back, 5 = forward. Left and right click are
never changed.

## Actions

A button click or a drag direction can run one of these actions:

- `none`, or `passthrough` (the button's original click)
- `back`, `forward`
- `mission-control`, `app-expose`, `show-desktop`
- `space-left`, `space-right`
- any key combination, for example `{ shortcut = "cmd+shift+4" }`

The named actions send the macOS default keyboard shortcuts (Ctrl+↑ Mission Control,
Ctrl+↓ App Exposé, Ctrl+←/→ Spaces, F11 Show Desktop). They only work while those
shortcuts are enabled under System Settings → Keyboard → Keyboard Shortcuts.

Buttons 4 and 5 default to a normal click, so each app handles back and forward as it does
without butterscroll. Choose Back or Forward in the Buttons pane to have butterscroll send
the shortcut instead.

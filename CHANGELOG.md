# Changelog

All notable changes to mfx are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Button remapping: buttons 3 and up (middle, back, forward, extra buttons) can send
  back/forward, Mission Control, App Exposé, Spaces left/right, Show Desktop, or any key
  combination.
- Smooth scrolling for notched mouse wheels, with acceleration and optional reversed
  direction. Trackpads and Magic Mouse are left untouched.
- Click and drag: hold a button and drag to scroll like two fingers on a trackpad, or to
  fire one action per direction (gestures).
- Menu-bar app with a settings window that follows the system light/dark appearance.
- The settings window explains every setting: a description under each one, labels at
  both ends of each slider, an ⓘ button on the scroll timing and acceleration settings
  that works through an example with your current values, and a Try It list at the end
  of Scrolling to feel a change right after you make it.

### Fixed

- Back and forward buttons did nothing in Dia once mfx ran: mfx swallowed them and sent
  `⌘[` and `⌘]`, which Dia ignores (it uses `⌘←` and `⌘→`). Buttons 4 and 5 now default
  to a normal click, so each app handles them as it does without mfx. An existing config
  keeps its own choice; set `click = "passthrough"` on buttons 4 and 5 to switch.
- On macOS 27.3 mfx stayed idle although it was allowed in System & App Protection
  Override: it waited for `AXIsProcessTrusted`, which ignores that grant. It now tries to
  create the event tap and judges by the result. `mfx check` does the same, and the
  messages and the menu item point to the right pane.

# butterscroll

A macOS menu-bar mouse enhancer, published as an open-source app for anyone to install.

## Distribution

**Release**:
A published, versioned build of butterscroll that anyone can download.
_Avoid_: build, drop, ship

**Version**:
The SemVer number (`X.Y.Z`) that identifies a Release; one number per Release, never reused.
_Avoid_: build number, revision

**Draft**:
A candidate Release that only the maintainer can see; it becomes a Release when the maintainer publishes it. Users are never offered a Draft.
_Avoid_: staging release, unpublished build

**Pre-release**:
A published build whose Version carries a pre-release suffix (e.g. `0.2.0-rc.1`), for people who opt in to test it. Never offered as an update.
_Avoid_: beta, nightly, RC build

## Settings

**Start at login**:
Whether macOS launches butterscroll when the user logs in. macOS owns this state; butterscroll only reads and changes it, never keeps its own copy.
_Avoid_: auto-start, launch agent, autostart

**First launch**:
The first time butterscroll runs for a given user on a Mac. It is the only moment butterscroll turns on Start at login by itself.
_Avoid_: first run, onboarding

**Access**:
The macOS grant that lets butterscroll intercept and change mouse events: System & App Protection Override on macOS 27.3 and later, Accessibility before. butterscroll has Access when macOS lets it create its event tap.
_Avoid_: permission, authorization, trust

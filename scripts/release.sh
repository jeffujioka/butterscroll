#!/usr/bin/env bash
# Release pipeline for mfx: build → package (sign, zip, verify) → publish (Draft) → promote.
# Maintainer guide: RELEASING.md. CI: .github/workflows/release.yml.
#
#   scripts/release.sh [--dry-run] [--tag vX.Y.Z[-pre]] [build|package|publish]
#   scripts/release.sh [--dry-run] promote vX.Y.Z[-pre]
#
# No subcommand runs build, package and publish in order.
# --dry-run: version from Cargo.toml unless --tag; skips tag/branch checks it can't run;
#            signs ad-hoc unless MFX_SIGN_ID is set; publish/promote only print gh commands.
#
# Environment:
#   MFX_SIGN_ID         SHA-1 of the release signing certificate (required unless --dry-run)
#   MFX_KEYCHAIN        keychain holding that certificate (optional; CI passes its temp keychain)
#   MFX_DEFAULT_BRANCH  branch every release tag must be on (default: origin/HEAD)
set -euo pipefail

# Names: the only place the app/bundle name lives in the pipeline (rename point).
APP_NAME=mfx
BUNDLE_ID=com.jeffujioka.mfx
MIN_MACOS=13.0
TARGETS=(aarch64-apple-darwin x86_64-apple-darwin)
EXPECTED_ARCHS="x86_64 arm64"
TAG_RE='^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-[0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*)?$'

ROOT=$(cd "$(dirname "$0")/.." && pwd)
DIST="$ROOT/dist"
APP="$DIST/$APP_NAME.app"
NOTES="$DIST/notes.md"
DR_FILE="$ROOT/packaging/designated-requirement.txt"
PLIST_TEMPLATE="$ROOT/packaging/Info.plist"

die() { printf 'release: error: %s\n' "$*" >&2; exit 1; }
warn() { printf 'release: WARNING: %s\n' "$*" >&2; }
log() { printf '\n== %s\n' "$*"; }

DRY_RUN=0
TAG_ARG=""
CMD=""
PROMOTE_TAG=""
while [ $# -gt 0 ]; do
  case "$1" in
    --dry-run) DRY_RUN=1 ;;
    --tag) [ $# -ge 2 ] || die "--tag needs a value"; TAG_ARG=$2; shift ;;
    -h | --help) sed -n '2,17p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    build | package | publish) [ -z "$CMD" ] || die "one subcommand at a time"; CMD=$1 ;;
    promote)
      [ -z "$CMD" ] || die "one subcommand at a time"
      [ $# -ge 2 ] || die "promote needs a tag"
      CMD=promote; PROMOTE_TAG=$2; shift ;;
    *) die "unknown argument: $1 (see --help)" ;;
  esac
  shift
done

# Prints a command; runs it unless --dry-run.
run() {
  if [ "$DRY_RUN" = 1 ]; then printf '[dry-run] would run:'; printf ' %q' "$@"; printf '\n'; else "$@"; fi
}

cargo_version() {
  awk -F'"' '/^\[package\]/ { p = 1; next } /^\[/ { p = 0 } p && /^version[[:space:]]*=/ { print $2; exit }' "$ROOT/Cargo.toml"
}

# Sets TAG, VERSION, NUMERIC, PRERELEASE, ZIP, TAG_IS_REAL.
resolve_version() {
  local tag=$1
  if [ -n "$tag" ]; then
    TAG=$tag; TAG_IS_REAL=1
  elif [ "${GITHUB_REF_TYPE:-}" = tag ]; then
    TAG=$GITHUB_REF_NAME; TAG_IS_REAL=1
  elif [ "$DRY_RUN" = 1 ]; then
    TAG="v$(cargo_version)"; TAG_IS_REAL=0
  else
    die "no release tag: run from a tag in CI, pass --tag, or use --dry-run"
  fi
  [[ $TAG =~ $TAG_RE ]] || die "tag $TAG doesn't match vMAJOR.MINOR.PATCH[-PRERELEASE] (no build metadata, no leading zeros)"
  VERSION=${TAG#v}
  NUMERIC=${VERSION%%-*}
  if [ "$VERSION" = "$NUMERIC" ]; then PRERELEASE=0; else PRERELEASE=1; fi
  ZIP="$APP_NAME-v$VERSION-universal.zip"
}

default_branch() {
  if [ -n "${MFX_DEFAULT_BRANCH:-}" ]; then echo "$MFX_DEFAULT_BRANCH"; return; fi
  git -C "$ROOT" symbolic-ref --quiet --short refs/remotes/origin/HEAD 2>/dev/null | sed 's|^origin/||'
}

# Tag == Cargo.toml, tag is HEAD, HEAD is on the default branch.
check_tag() {
  local cv
  cv=$(cargo_version)
  [ "$VERSION" = "$cv" ] || die "tag $TAG doesn't match Cargo.toml version $cv"
  if [ "$TAG_IS_REAL" = 0 ]; then
    warn "dry run without a tag: skipping tag and branch checks (version $VERSION from Cargo.toml)"
    return
  fi
  if git -C "$ROOT" rev-parse -q --verify "refs/tags/$TAG" >/dev/null; then
    [ "$(git -C "$ROOT" rev-parse "$TAG^{commit}")" = "$(git -C "$ROOT" rev-parse HEAD)" ] ||
      die "HEAD is not the commit tagged $TAG"
  elif [ "$DRY_RUN" = 1 ]; then
    warn "tag $TAG doesn't exist yet: skipping tag and branch checks"
    return
  else
    die "tag $TAG doesn't exist"
  fi
  local branch
  branch=$(default_branch)
  if [ -z "$branch" ] || ! git -C "$ROOT" rev-parse -q --verify "refs/remotes/origin/$branch" >/dev/null; then
    [ "$DRY_RUN" = 1 ] || die "can't find origin's default branch (set MFX_DEFAULT_BRANCH)"
    warn "no remote default branch: skipping the branch check"
    return
  fi
  git -C "$ROOT" merge-base --is-ancestor HEAD "origin/$branch" ||
    die "tag $TAG is not on origin/$branch"
}

# Writes the CHANGELOG section for this version to $1.
# Final: "## [X.Y.Z] - YYYY-MM-DD"; pre-release: "## [Unreleased]". Must not be empty.
extract_notes() {
  local out=$1 header
  if [ "$PRERELEASE" = 1 ]; then header='## [Unreleased]'; else header="## [$VERSION] - "; fi
  awk -v h="$header" -v pre="$PRERELEASE" '
    BEGIN { found = 0; inside = 0; n = 0 }
    /^## \[/ {
      if (inside) exit
      if ((pre == 1 && $0 == h) || (pre == 0 && index($0, h) == 1 && $0 ~ /- [0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9][[:space:]]*$/)) {
        found = 1; inside = 1; next
      }
    }
    inside { lines[++n] = $0 }
    END {
      if (!found) exit 3
      first = 1; while (first <= n && lines[first] !~ /[^[:space:]]/) first++
      last = n; while (last >= first && lines[last] !~ /[^[:space:]]/) last--
      for (i = first; i <= last; i++) print lines[i]
    }
  ' "$ROOT/CHANGELOG.md" >"$out" || {
    if [ "$PRERELEASE" = 1 ]; then die "CHANGELOG.md has no '## [Unreleased]' section"; fi
    die "CHANGELOG.md has no '## [$VERSION] - YYYY-MM-DD' section"
  }
  grep -q '[^[:space:]]' "$out" || die "CHANGELOG.md section for $VERSION is empty"
}

# Installs the toolchain pinned in rust-toolchain.toml with both targets
# (rustup >= 1.28 no longer installs it implicitly).
ensure_toolchain() {
  local channel
  channel=$(awk -F'"' '/^channel[[:space:]]*=/ { print $2 }' "$ROOT/rust-toolchain.toml")
  [ -n "$channel" ] || die "no channel in rust-toolchain.toml"
  log "rust $channel"
  rustup toolchain install "$channel" --no-self-update --profile minimal --component rustfmt,clippy \
    --target "$(IFS=,; echo "${TARGETS[*]}")"
}

log_tools() {
  log "toolchain"
  sw_vers || true
  xcodebuild -version 2>/dev/null || xcode-select -p || true
  (cd "$ROOT" && rustc -vV)
}

ensure_rosetta() {
  arch -x86_64 /usr/bin/true 2>/dev/null && return
  if [ "${CI:-}" = true ]; then
    log "installing Rosetta"
    sudo softwareupdate --install-rosetta --agree-to-license
  else
    die "Rosetta is required to test the x86_64 slice: softwareupdate --install-rosetta"
  fi
}

render_plist() {
  local short=$1 bundle=$2 out=$3
  sed -e "s|@APP_NAME@|$APP_NAME|g" -e "s|@BUNDLE_ID@|$BUNDLE_ID|g" -e "s|@MIN_MACOS@|$MIN_MACOS|g" \
    -e "s|@SHORT_VERSION@|$short|g" -e "s|@BUNDLE_VERSION@|$bundle|g" "$PLIST_TEMPLATE" >"$out"
  ! grep -q '@[A-Z_]*@' "$out" || die "unreplaced placeholder in Info.plist"
}

# Architecture, deployment target and version of the executable inside an app bundle.
check_executable() {
  local exe=$1 archs out
  archs=$(lipo -archs "$exe")
  [ "$archs" = "$EXPECTED_ARCHS" ] || die "$exe architectures are '$archs', expected '$EXPECTED_ARCHS'"
  local minos
  minos=$(vtool -show-build "$exe" | awk '/minos/ { print $2 }' | sort -u)
  [ "$minos" = "$MIN_MACOS" ] || die "$exe minimum macOS is '$(tr '\n' ' ' <<<"$minos")', expected $MIN_MACOS on every slice"
  for a in arm64 x86_64; do
    out=$(arch "-$a" "$exe" --version) || die "$exe --version failed on $a"
    [ "$out" = "$APP_NAME $VERSION" ] || die "$exe --version on $a printed '$out', expected '$APP_NAME $VERSION'"
  done
}

check_plist() {
  local plist=$1 v
  plutil -lint "$plist" >/dev/null || die "$plist is not a valid property list"
  v=$(plutil -extract CFBundleShortVersionString raw "$plist")
  [ "$v" = "$VERSION" ] || die "CFBundleShortVersionString is '$v', expected '$VERSION'"
  v=$(plutil -extract CFBundleVersion raw "$plist")
  [ "$v" = "$NUMERIC" ] || die "CFBundleVersion is '$v', expected '$NUMERIC'"
  v=$(plutil -extract CFBundleIdentifier raw "$plist")
  [ "$v" = "$BUNDLE_ID" ] || die "CFBundleIdentifier is '$v', expected '$BUNDLE_ID'"
}

cmd_build() {
  ensure_toolchain
  log_tools
  log "checks for $TAG"
  check_tag
  [ -f "$PLIST_TEMPLATE" ] || die "missing $PLIST_TEMPLATE"
  rm -rf "$DIST"
  mkdir -p "$DIST"
  extract_notes "$NOTES"
  ensure_rosetta

  export MACOSX_DEPLOYMENT_TARGET=$MIN_MACOS
  local t bins=()
  for t in "${TARGETS[@]}"; do
    log "test $t"
    (cd "$ROOT" && cargo test --locked --target "$t")
    log "build $t"
    (cd "$ROOT" && cargo build --release --locked --target "$t")
    bins+=("$ROOT/target/$t/release/$APP_NAME")
  done

  log "assemble $APP_NAME.app (unsigned)"
  mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
  lipo -create -output "$APP/Contents/MacOS/$APP_NAME" "${bins[@]}"
  render_plist "$VERSION" "$NUMERIC" "$APP/Contents/Info.plist"
  check_plist "$APP/Contents/Info.plist"
  check_executable "$APP/Contents/MacOS/$APP_NAME"
  echo "built: $APP"
}

cmd_package() {
  log "checks for $TAG"
  check_tag
  [ -d "$APP" ] || die "missing $APP: run 'release.sh build' first"
  [ -s "$NOTES" ] || die "missing $NOTES: run 'release.sh build' first"

  local sign_id=${MFX_SIGN_ID:-} dr=""
  if [ -z "$sign_id" ]; then
    [ "$DRY_RUN" = 1 ] || die "MFX_SIGN_ID is not set: real releases must be signed with the release certificate"
    warn "MFX_SIGN_ID not set: signing AD-HOC. This bundle could never update an installed mfx."
    sign_id=-
  elif [ -f "$DR_FILE" ]; then
    dr=$(cat "$DR_FILE")
  elif [ "$DRY_RUN" = 1 ]; then
    warn "$DR_FILE missing: skipping designated-requirement checks"
  else
    die "missing $DR_FILE (see RELEASING.md, one-time setup)"
  fi

  log "sign"
  local kc=()
  [ -z "${MFX_KEYCHAIN:-}" ] || kc=(--keychain "$MFX_KEYCHAIN")
  codesign --force --timestamp=none --sign "$sign_id" ${kc[@]+"${kc[@]}"} --identifier "$BUNDLE_ID" "$APP"

  log "zip"
  rm -f "$DIST/$ZIP" "$DIST/$ZIP.sha256"
  (cd "$DIST" && ditto -c -k --keepParent "$APP_NAME.app" "$ZIP" && shasum -a 256 "$ZIP" >"$ZIP.sha256")

  log "verify $ZIP"
  (cd "$DIST" && shasum -a 256 -c "$ZIP.sha256")
  local roots
  roots=$(zipinfo -1 "$DIST/$ZIP" | cut -d/ -f1 | sort -u)
  [ "$roots" = "$APP_NAME.app" ] || die "zip root must be exactly $APP_NAME.app/, found: $(tr '\n' ' ' <<<"$roots")"

  # Verify the unzipped copy, exactly as install.sh and the updater see it.
  local tmp="$DIST/.verify"
  rm -rf "$tmp"
  mkdir -p "$tmp"
  ditto -x -k "$DIST/$ZIP" "$tmp"
  local app="$tmp/$APP_NAME.app"
  check_plist "$app/Contents/Info.plist"
  check_executable "$app/Contents/MacOS/$APP_NAME"
  local ident
  ident=$(codesign -d --verbose=1 "$app" 2>&1 | awk -F= '/^Identifier=/ { print $2 }')
  [ "$ident" = "$BUNDLE_ID" ] || die "signed identifier is '$ident', expected '$BUNDLE_ID'"
  if [ -n "$dr" ]; then
    local embedded
    embedded=$(codesign -d -r- "$app" 2>/dev/null | sed -n 's/^designated => //p')
    [ "$embedded" = "$dr" ] || die "embedded designated requirement differs from $DR_FILE:
  embedded: $embedded
  expected: $dr"
    # Exactly what install.sh and the in-app updater run before installing.
    codesign --verify --deep --strict -R="$dr" "$app" || die "unzipped app fails the pinned designated requirement"
  else
    codesign --verify --deep --strict "$app"
  fi
  rm -rf "$tmp"
  echo "packaged: $DIST/$ZIP"
}

# Prints "none", "draft" or "published" for a tag.
release_state() {
  local out
  if out=$(gh release view "$1" --json isDraft -q .isDraft 2>&1); then
    if [ "$out" = true ]; then echo draft; else echo published; fi
  elif grep -qi 'release not found' <<<"$out"; then
    echo none
  else
    die "gh release view $1: $out"
  fi
}

# Prints the tag of the release GitHub marks as latest, or nothing if there is none.
latest_tag() {
  local out
  if out=$(gh release view --json tagName -q .tagName 2>&1); then
    echo "$out"
  elif ! grep -qi 'release not found' <<<"$out"; then
    die "gh release view (latest): $out"
  fi
}

# Fails unless the tag is a Draft carrying both release assets.
require_complete_draft() {
  [ "$(release_state "$TAG")" = draft ] || die "$TAG is not a Draft"
  local assets
  assets=$(gh release view "$TAG" --json assets -q '.assets[].name')
  { grep -qxF "$ZIP" <<<"$assets" && grep -qxF "$ZIP.sha256" <<<"$assets"; } ||
    die "Draft $TAG lacks $ZIP or $ZIP.sha256"
}

cmd_publish() {
  log "publish Draft for $TAG"
  [ -f "$DIST/$ZIP" ] && [ -f "$DIST/$ZIP.sha256" ] && [ -s "$NOTES" ] ||
    die "missing $ZIP, its .sha256 or notes.md in dist/: run build and package first"
  local pre=()
  [ "$PRERELEASE" = 0 ] || pre=(--prerelease)
  local state
  if [ "$DRY_RUN" = 1 ]; then
    state=none
    echo "[dry-run] assuming no release exists for $TAG"
  else
    state=$(release_state "$TAG")
  fi
  case "$state" in
    none)
      run gh release create "$TAG" --draft --verify-tag ${pre[@]+"${pre[@]}"} \
        --title "$APP_NAME $TAG" --notes-file "$NOTES" "$DIST/$ZIP" "$DIST/$ZIP.sha256" ;;
    draft)
      run gh release upload "$TAG" --clobber "$DIST/$ZIP" "$DIST/$ZIP.sha256"
      run gh release edit "$TAG" ${pre[@]+"${pre[@]}"} --notes-file "$NOTES" ;;
    published)
      die "$TAG is already published; a published Version is never rebuilt. Ship a new version." ;;
  esac
  echo "Draft ready. Smoke-test it, then: scripts/release.sh promote $TAG"
}

cmd_promote() {
  resolve_version "$PROMOTE_TAG"
  log "promote $TAG"
  if [ "$DRY_RUN" = 0 ]; then
    require_complete_draft
  fi
  # Pre-releases are never latest; a final is latest only if it is the highest final Version.
  local latest=false
  if [ "$PRERELEASE" = 0 ]; then
    local current=""
    if [ "$DRY_RUN" = 1 ]; then
      echo "[dry-run] assuming no published Release yet"
    else
      current=$(latest_tag)
    fi
    if [ -z "$current" ]; then
      latest=true
    else
      local cur=${current#v}
      cur=${cur%%-*}
      if [ "$cur" != "$NUMERIC" ] && [ "$(printf '%s\n%s\n' "$cur" "$NUMERIC" | sort -V | tail -1)" = "$NUMERIC" ]; then
        latest=true
      fi
      echo "current latest: $current → $TAG becomes latest: $latest"
    fi
  fi
  run gh release edit "$TAG" --draft=false "--latest=$latest"
}

if [ "$CMD" = promote ]; then
  cmd_promote
  exit 0
fi

resolve_version "$TAG_ARG"
[ "$DRY_RUN" = 0 ] || warn "DRY RUN: nothing will be published"
case "$CMD" in
  build) cmd_build ;;
  package) cmd_package ;;
  publish) cmd_publish ;;
  "") cmd_build; cmd_package; cmd_publish ;;
esac

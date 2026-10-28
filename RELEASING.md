# Releasing mfx

Releases are built by GitHub Actions (`.github/workflows/release.yml`) when a `v*` tag is
pushed. Every step lives in `scripts/release.sh`, so the same pipeline runs locally with
`--dry-run`. CI only ever creates a **Draft**; you publish it with `scripts/release.sh promote`
after a smoke test.

```
tag push ─▶ build (no secrets) ─▶ sign-publish (you approve) ─▶ Draft ─▶ smoke test ─▶ promote
```

## One-time setup

### 1. Release signing certificate

See [ADR 0001](docs/adr/0001-self-signed-release-cert.md). One self-signed certificate signs
every Release, valid for 20 years. **Back up the `.p12` and its password in two places.**
Losing it, or leaking the key, forces every user to grant Accessibility again and reinstall
by hand.

```sh
cd "$(mktemp -d)"
openssl req -x509 -newkey rsa:3072 -nodes -days 7300 \
  -keyout key.pem -out cert.pem -subj "/CN=mfx Release Signing" \
  -addext "basicConstraints=critical,CA:false" \
  -addext "keyUsage=critical,digitalSignature" \
  -addext "extendedKeyUsage=critical,codeSigning"
# -legacy is required: `security import` rejects the OpenSSL 3+ default .p12 format
# ("MAC verification failed during PKCS12 import").
openssl pkcs12 -export -legacy -inkey key.pem -in cert.pem -name "mfx Release Signing" -out mfx-release.p12
openssl x509 -in cert.pem -noout -fingerprint -sha1 | cut -d= -f2 | tr -d : | tr 'A-F' 'a-f'   # → SHA-1
```

Import it into your login keychain to sign local dry runs:

```sh
security import mfx-release.p12 -k ~/Library/Keychains/login.keychain-db -T /usr/bin/codesign
```

Then store `mfx-release.p12` and its password in your backups and delete `key.pem`.

### 2. Pin the designated requirement

`install.sh`, the in-app updater and the release pipeline all accept only an app whose
signature satisfies this requirement. Commit it once:

```sh
printf 'identifier "com.jeffujioka.mfx" and certificate leaf = H"%s"\n' <lowercase-sha1> \
  > packaging/designated-requirement.txt
```

Real releases fail without this file. Check it with a signed dry run:
`MFX_SIGN_ID=<sha1> scripts/release.sh --dry-run`. The output must show no
"skipping designated-requirement checks" warning.

### 3. GitHub environment `release`

In the repository's Settings → Environments → **New environment** `release`:

- **Required reviewers**: yourself. Leave "Prevent self-review" **off**.
- **Deployment branches and tags**: "Selected branches and tags" → add a **tag** rule `v*`.
- **Environment secrets**:
  - `MFX_CERT_P12`: `base64 -i mfx-release.p12 | pbcopy`, then paste.
  - `MFX_CERT_PASSWORD`: the `.p12` password.
- **Environment variables**: `MFX_SIGN_ID` = the certificate's SHA-1.

Only the `sign-publish` job uses this environment, and it never runs `cargo`. Dependency
build scripts and tests run in `build`, which has no secrets.

### 4. First CI run

Make the first tag a pre-release (`v0.1.0-rc.1`, see below). It exercises the whole path,
including the keychain import on the runner, without becoming the latest Release.

## Releasing a version

1. **CHANGELOG**: move everything under `## [Unreleased]` into a new
   `## [X.Y.Z] - YYYY-MM-DD` section. Leave an empty `## [Unreleased]` above it. That section
   becomes the Release notes, and the pipeline fails if it is missing or empty.
2. **Version**: set `version = "X.Y.Z"` in `Cargo.toml`. Run `cargo check` so `Cargo.lock`
   follows (builds use `--locked`). Commit both on the default branch and push.
3. **Dry run**: `scripts/release.sh --dry-run`. Add `MFX_SIGN_ID=<sha1>` to sign with the real
   certificate and run the designated-requirement checks. It runs every check it can, prints
   the `gh` commands it would run, and publishes nothing.
4. **Tag**: `git tag -a vX.Y.Z -m "mfx vX.Y.Z" && git push origin vX.Y.Z`.
5. **Approve**: Actions → the `release` run → **Review deployments** → approve `release`.
6. **Smoke test the Draft** on a second macOS user account:
   ```sh
   gh release download vX.Y.Z -D ~/mfx-vX.Y.Z
   (cd ~/mfx-vX.Y.Z && shasum -a 256 -c mfx-vX.Y.Z-universal.zip.sha256)
   MFX_INSTALL_ZIP=~/mfx-vX.Y.Z/mfx-vX.Y.Z-universal.zip bash install.sh
   ```
   Check the Accessibility prompt, start at login, and the version in the menu. Test the
   update path from the previous Release with the updater's `MFX_UPDATE_URL` override.
7. **Publish**: `scripts/release.sh promote vX.Y.Z`. It refuses unless the Draft has both
   assets. It marks the Release *latest* only when X.Y.Z is higher than the current latest,
   so a backport (e.g. 0.1.5 after 0.2.0) never becomes what `install.sh` and the updater
   install.

## Pre-releases

Tags with a suffix (`v0.2.0-rc.1`) become GitHub pre-releases. The updater never offers them,
and `promote` never marks them latest.

| | Final `v0.2.0` | Pre-release `v0.2.0-rc.1` |
|---|---|---|
| `Cargo.toml` version | `0.2.0` | `0.2.0-rc.1` |
| Release notes from | `## [0.2.0] - date` | `## [Unreleased]` (not moved) |
| Asset | `mfx-v0.2.0-universal.zip` | `mfx-v0.2.0-rc.1-universal.zip` |
| `CFBundleShortVersionString` / `CFBundleVersion` | `0.2.0` / `0.2.0` | `0.2.0-rc.1` / `0.2.0` |

Steps: set the pre-release version in `Cargo.toml`, keep the notes under `[Unreleased]`, then
follow steps 2–7 above with the pre-release tag. For the final release, follow the full list.

## Release asset contract

`install.sh` and the in-app updater depend on these. Change them only together.

- Assets: `mfx-v<version>-universal.zip` and `mfx-v<version>-universal.zip.sha256`
  (`shasum -a 256` format, bare file name).
- The zip, made with `ditto -c -k --keepParent`, contains exactly one `mfx.app/` at its root.
- The app satisfies `packaging/designated-requirement.txt`:
  `codesign --verify --deep --strict -R=<requirement>`.
- `CFBundleShortVersionString` equals the tag without `v`. `mfx --version` prints
  `mfx <version>`. Both slices (arm64, x86_64) target macOS 13.0.
- Signed with `--timestamp=none` and no hardened runtime. Not notarized (ADR 0001).

The pipeline checks all of these on the unzipped copy before uploading anything.

## When something fails

- **Re-running a run** for the same tag replaces the Draft's assets and notes. A published
  Release is never modified: ship the next patch version instead.
- **Never move a pushed tag.** Fix the problem on the default branch, delete the broken Draft
  in the GitHub UI, and tag the next patch or `-rc.N` version.
- **HTTP 404 when creating or editing the Release**: GitHub refuses when the tagged commit's
  `.github/workflows/` differs from the default branch's. Tag a new version from the current
  default branch.
- **`MAC verification failed during PKCS12 import`**: the `.p12` isn't in legacy format.
  Re-export it with `openssl pkcs12 -export -legacy` and update `MFX_CERT_P12`.
- **`errSecInternalComponent` from codesign**: the temporary keychain is locked. The workflow
  unlocks it and sets a 6-hour lock timeout, so check that the import step ran.
- **Embedded designated requirement differs**: the signing certificate isn't the one pinned
  in `packaging/designated-requirement.txt`. Do not "fix" this by editing the file: users only
  accept updates signed with the pinned certificate (ADR 0001).

## Maintenance

- **Rust**: `rust-toolchain.toml` pins an exact version. Bump it in its own commit.
- **Runner**: pinned to `macos-26`. Apple keeps Rosetta "available through macOS 27". The
  pipeline runs the x86_64 slice under Rosetta, so revisit those checks
  (`ensure_rosetta`, `check_executable` in `scripts/release.sh`) before moving the runner
  past macOS 27.
- **Actions** are pinned to commit SHAs with the version in a comment. Update the SHA and the
  comment together.

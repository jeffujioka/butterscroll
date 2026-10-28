#!/usr/bin/env bash
# Build the release binary and code-sign it.
# macOS ties the Accessibility grant to the signature: with the default ad-hoc
# signature ("-") every rebuild needs re-granting. Set MFX_SIGN_ID to the name of
# a stable certificate in your keychain (Developer ID or self-signed code-signing
# cert) to keep the grant across rebuilds.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release
codesign --force --sign "${MFX_SIGN_ID:--}" --identifier com.jeffujioka.mfx target/release/mfx
codesign --display --verbose=2 target/release/mfx 2>&1 | grep -E '^(Identifier|Signature|Authority)'
echo "built: $(pwd)/target/release/mfx"

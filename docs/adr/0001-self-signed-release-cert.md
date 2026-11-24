# Releases are signed with one long-lived self-signed certificate

butterscroll is published without the paid Apple Developer Program, so Releases are not notarized and the official Homebrew cask is not an option. Every Release is instead signed with a single self-signed code-signing certificate (20-year validity) whose exported `.p12` is kept as a backup. That certificate does two jobs: macOS ties the Accessibility grant to it, so the grant survives updates, and both `install.sh` and the in-app updater only install an app signed with the same certificate. Losing or replacing it forces every user to grant Accessibility again and reinstall by hand.

## Considered Options

- **Developer ID + notarization**: no Gatekeeper prompt and Homebrew-eligible, but costs $99/year.
- **Ad-hoc signing**: free, but every update loses the Accessibility grant and the updater has no identity to check.
- **Apple Development certificate**: free, but expires yearly (each renewal breaks the grant) and is meant for development, not distribution.

## Consequences

- Self-signed signatures carry no secure timestamp, so the validity is set long enough (20 years) that expiry never forces a rotation during the project's life.
- A self-signed certificate cannot be revoked. If the private key leaks, the only response is rotating to a new certificate, with the same cost as losing it.

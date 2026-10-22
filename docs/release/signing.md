# Signing and Notarization

CI produces **unsigned development builds only**. They are for testing and must not be presented as public releases.

## Windows

Public MSI/NSIS distribution requires a publisher-controlled code-signing certificate and protected CI access to its private key and password. Configure signing through Tauri's supported Windows signing variables, verify Authenticode after packaging, and timestamp signatures. Never commit certificates or passwords.

## macOS

Public DMG/app distribution requires an Apple Developer ID Application identity, a publisher-controlled keychain certificate, Apple notarization credentials, hardened runtime signing, notarization submission, and stapling. Verify both `codesign` and Gatekeeper on a clean machine.

## Release gate

Signing remains blocked until the publisher supplies credentials through protected CI secrets and approves the final application identifier and product name. Linux packages do not remove the Windows/macOS signing block. Record the signer identity, artifact hash, verification output, and notarization request in the private release record without exposing secrets.

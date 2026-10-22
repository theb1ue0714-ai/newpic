# NewPic

NewPic is an original, clean-room, cross-platform screenshot utility for
Windows, macOS, and Linux. It uses a Tauri 2 desktop shell, a Rust backend,
and a TypeScript settings interface.

## Product scope

Phase 1 targets a dependable capture workflow:

- configurable F7 region capture
- configurable F8 repeat-last-region capture
- configurable F9 fixed-size region capture (800×600 by default)
- configurable F10 active-window capture
- automatic PNG saving to the current user's `Downloads/NewPic` folder
- collision-free `%c` sequence names beginning at `001.png`
- simultaneous image output to the system clipboard
- per-user login startup and tray operation
- permission, shortcut, storage, and platform-capability diagnostics

Image editing, scrolling capture, recording, cloud sharing, and auxiliary
screen tools are intentionally deferred to later, separately approved phases.

## Status

**Phase 1 release candidate; acceptance evidence pending.**
The capture pipeline, overlays, settings UI, diagnostics, tray, shortcuts, and
login startup are implemented. Automated local gates run on Linux; Windows and
macOS verification awaits successful CI runs. Interactive desktop evidence and public
Windows/macOS signing remain release blockers.

## Build prerequisites

Development and packaging require:

- Git
- a current stable Rust toolchain with Cargo
- a Node.js 24.15-compatible pinned toolchain and npm 11
- npm workspaces with the committed `package-lock.json`; install with
  `npm install`
- Tauri 2 platform prerequisites for the host operating system
- Windows SDK and WebView2 tooling for Windows packages
- Xcode command-line tools for macOS packages
- WebKitGTK, GTK, and related Tauri system libraries for Linux packages

Platform signing and notarization credentials are required only for public
distribution and must never be committed. See the implementation plan for the
exact commands as each workspace component is introduced.

## Verification

Run the complete local gate from the repository root:

```bash
npm run prepare-icons --workspace=@newpic/desktop
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm test -- --run
npm run typecheck
npm run build --workspace=@newpic/desktop
bash scripts/check-repo-contract.sh
bash scripts/verify-default-preset.sh
```

GitHub CI repeats the checks on current Ubuntu, Windows, and macOS runners.
The installer workflow produces clearly labeled unsigned development bundles
only on manual dispatch or `v*` tags.

## Privacy

Phase 1 has no telemetry. Screenshots and clipboard data remain local unless a
future sharing feature is separately designed and enabled. Diagnostic logs
contain operational metadata only and must never contain captured pixels or
clipboard contents.

## Project documents

- [Approved NewPic design specification](docs/superpowers/specs/2026-10-22-newpic-design.md)
- [NewPic MVP implementation plan](docs/plans/2026-10-22-newpic-mvp-implementation.md)
- [Visual design system](DESIGN.md)
- [Clean-room policy](docs/architecture/clean-room-policy.md)
- [REA environment baseline](research/rea/environment-baseline.md)
- [Manual release matrix](docs/testing/manual-release-matrix.md)
- [Linux smoke test](docs/testing/linux-smoke-test.md)
- [Signing and notarization](docs/release/signing.md)
- [Known platform limitations](docs/release/known-platform-limitations.md)

## License

Original NewPic code and documentation are licensed under the [MIT License](LICENSE).
Third-party tools, dependencies, and reference materials retain their own
licenses and notices; incorporated notices are listed in
[`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md). NewPic is not affiliated
with PicPick or Raycast.

# NewPic MVP Implementation Plan

> **For Claude:** Use `${SUPERPOWERS_SKILLS_ROOT}/skills/collaboration/executing-plans/SKILL.md` to implement this plan task-by-task.

**Goal:** Build a Tauri 2 desktop screenshot application whose approved F7/F8/F9/F10 preset, automatic PNG saving, clipboard output, diagnostics, and login startup work across Windows, macOS, and supported Linux sessions.

**Architecture:** A Rust workspace owns settings, naming, geometry, capture contracts, image output, and platform adapters. A small Tauri/React application owns tray integration, global shortcuts, transparent selection overlays, settings, and diagnostics. `xcap` provides the common native capture primitive while adapters and capability reporting preserve explicit Windows, macOS, X11, and Wayland behavior.

**Tech Stack:** Node.js 24.15, npm 11, Rust 1.99, Tauri 2.12, React 19, TypeScript, Vite, Vitest, Testing Library, Cargo test, `xcap` 0.9, `image` 0.25, `arboard` 3.7, official Tauri global-shortcut/autostart/notification plugins.

**Design reference:** `docs/superpowers/specs/2026-10-22-newpic-design.md`

---

## Execution rules

- Work in a dedicated `feature/newpic-mvp` worktree created with `@using-git-worktrees`.
- Use `@test-driven-development` for every feature task and `@verification-before-completion` before final claims.
- Use `apply_patch` for hand-authored files. Package managers may generate lockfiles and dependency directories.
- Keep commits limited to one task. Do not mix REA evidence, application code, and unrelated environment changes.
- Never claim Windows or macOS runtime completion from Linux-only evidence. Those platforms pass only after their native CI jobs and manual smoke checks pass.
- Treat REA output as research evidence, not source code to copy.

## Pinned starting versions

- `rea-agents@6.7.0`
- `@tauri-apps/cli@2.12.4`
- `@tauri-apps/api@2.12.4`
- `tauri = 2.12.5`
- `tauri-build = 2.7.4`
- `tauri-plugin-global-shortcut = 2.4.1`
- `tauri-plugin-autostart = 2.7.0`
- `tauri-plugin-notification = 2.7.0`
- `xcap = 0.9.8`
- `arboard = 3.7.0`
- `image = 0.25.10`
- `directories = 6.0.0`
- `serde = 1.0.229`
- `serde_json = 1.0.154`
- `thiserror = 2.0.21`
- `tempfile = 3.27.0`

Update a pinned version only in a dedicated dependency commit after reading its official changelog and rerunning the full verification matrix.

---

### Task 1: Install and verify REA globally

**Files:**
- Create: `research/rea/environment-baseline.md`
- Create: `research/rea/README.md`

**Step 1: Record the pre-install baseline**

Run:

```bash
node --version
npm --version
rustc --version
command -v ps
npm view rea-agents version
```

Expected: Node is `v24.15.0` or another REA-supported release, `ps` exists, and the registry reports `6.7.0` unless a newer version has been deliberately approved.

Use `apply_patch` to record commands, output, date, host OS, and why no Hopper/Ghidra installation is required for the JavaScript/desktop workflow.

**Step 2: Install the pinned global CLI**

Run:

```bash
npm install --global rea-agents@6.7.0
rea --version
```

Expected: installation exits 0 and `rea --version` identifies 6.7.0.

**Step 3: Preview the exact Codex registration**

Run:

```bash
rea setup --client codex --dry-run --json > /tmp/newpic-rea-setup-plan.json
python3 -m json.tool /tmp/newpic-rea-setup-plan.json >/dev/null
```

Expected: valid JSON and no configuration change. Review it for only the `codex` client and the bundled REA workflow skill.

**Step 4: Apply non-interactively and diagnose**

Run:

```bash
rea setup --client codex --yes --json
rea doctor --client codex --json
```

Expected: setup and doctor return success. If the active session does not discover the new MCP server, record that a Codex restart is required; do not fabricate tool availability.

**Step 5: Create the research rules**

`research/rea/README.md` must require target source URL, version, SHA-256, confirmed facts, inference, unknowns, and clean-room notes for every investigation. It must prohibit copying decompiled code, icons, branding, or proprietary assets.

**Step 6: Commit**

```bash
git add research/rea
git commit -m "chore: verify REA analysis environment"
```

---

### Task 2: Add repository rules and the visual design system

**Files:**
- Create: `AGENTS.md`
- Create: `DESIGN.md`
- Create: `README.md`
- Create: `LICENSE`
- Create: `.gitignore`
- Create: `docs/architecture/clean-room-policy.md`

**Step 1: Add a documentation assertion test**

Create `scripts/check-repo-contract.sh` with checks that fail when `AGENTS.md` does not reference `/home/mint/AGENT.md`, `DESIGN.md`, the approved specification, and this plan; or when `DESIGN.md` does not credit its source and state the NewPic adaptations.

Run:

```bash
bash scripts/check-repo-contract.sh
```

Expected: FAIL because the referenced files do not exist.

**Step 2: Add the sourced design reference**

Read the MIT-licensed Raycast reference at:

```text
https://github.com/VoltAgent/awesome-design-md/blob/main/design-md/raycast/DESIGN.md
```

Create root `DESIGN.md` from that actual source, then append a clearly separated NewPic adaptation section:

- utility-window layout rather than a marketing page
- system font stack rather than downloading Inter
- light and dark token pairs
- 8px radius, hairline borders, restrained blue capture accent
- no decorative gradient hero treatment
- minimum 44px interactive targets where practical
- `prefers-reduced-motion`
- WCAG AA text contrast

**Step 3: Add project instructions**

`AGENTS.md` must state that `/home/mint/AGENT.md`, root `DESIGN.md`, the approved specification, clean-room policy, TDD, and verification gates apply to all work.

**Step 4: Add repository metadata**

- `README.md`: product scope, Phase 1 status, build prerequisites, privacy statement, and links to spec/plan.
- `LICENSE`: MIT for original NewPic code.
- `.gitignore`: Node, Rust, Tauri artifacts, OS files, signing files, `.env*`, REA captures, and recovered proprietary binaries.
- `docs/architecture/clean-room-policy.md`: sanitized behavior notes only and no proprietary artifacts in Git.

**Step 5: Run and commit**

```bash
bash scripts/check-repo-contract.sh
git add AGENTS.md DESIGN.md README.md LICENSE .gitignore scripts/check-repo-contract.sh docs/architecture/clean-room-policy.md
git commit -m "docs: establish NewPic repository standards"
```

Expected: contract check passes.

---

### Task 3: Scaffold the Rust workspace and Tauri application

**Files:**
- Create: `Cargo.toml`
- Create: `rust-toolchain.toml`
- Create: `package.json`
- Create: `package-lock.json` (generated)
- Create: `apps/newpic/package.json`
- Create: `apps/newpic/vite.config.ts`
- Create: `apps/newpic/tsconfig.json`
- Create: `apps/newpic/index.html`
- Create: `apps/newpic/src/main.tsx`
- Create: `apps/newpic/src/App.tsx`
- Create: `apps/newpic/src/styles.css`
- Create: `apps/newpic/src-tauri/Cargo.toml`
- Create: `apps/newpic/src-tauri/build.rs`
- Create: `apps/newpic/src-tauri/src/main.rs`
- Create: `apps/newpic/src-tauri/src/lib.rs`
- Create: `apps/newpic/src-tauri/tauri.conf.json`
- Create: `apps/newpic/src-tauri/capabilities/default.json`
- Create: `crates/newpic-core/Cargo.toml`
- Create: `crates/newpic-core/src/lib.rs`
- Create: `crates/newpic-platform/Cargo.toml`
- Create: `crates/newpic-platform/src/lib.rs`
- Create: `crates/newpic-windows/Cargo.toml`
- Create: `crates/newpic-windows/src/lib.rs`
- Create: `crates/newpic-macos/Cargo.toml`
- Create: `crates/newpic-macos/src/lib.rs`
- Create: `crates/newpic-linux/Cargo.toml`
- Create: `crates/newpic-linux/src/lib.rs`
- Create: `crates/newpic-testing/Cargo.toml`
- Create: `crates/newpic-testing/src/lib.rs`

**Step 1: Write a failing workspace smoke test**

Create `crates/newpic-core/tests/workspace_smoke.rs`:

```rust
#[test]
fn crate_reports_product_name() {
    assert_eq!(newpic_core::PRODUCT_NAME, "NewPic");
}
```

Run:

```bash
cargo test -p newpic-core --test workspace_smoke
```

Expected: FAIL because the workspace and constant do not exist.

**Step 2: Create the minimal workspace**

The root Cargo workspace includes all Rust paths above, uses resolver 2, and centralizes the pinned shared dependencies. `newpic_core::PRODUCT_NAME` is the only initial implementation.

The npm workspace contains `apps/newpic` and scripts:

```json
{
  "scripts": {
    "dev": "npm --workspace @newpic/desktop run tauri dev",
    "build": "npm --workspace @newpic/desktop run tauri build",
    "test": "npm --workspace @newpic/desktop test",
    "typecheck": "npm --workspace @newpic/desktop run typecheck"
  }
}
```

Use React/Vite and install the pinned Tauri packages. The starter window must render only “NewPic 설정”.

**Step 3: Verify the scaffold**

Run:

```bash
npm install
cargo test --workspace
npm test -- --run
npm run typecheck
npm run build --workspace=@newpic/desktop
```

Expected: all commands exit 0 and Vite produces `apps/newpic/dist`.

**Step 4: Commit**

```bash
git add Cargo.toml Cargo.lock rust-toolchain.toml package.json package-lock.json apps crates
git commit -m "build: scaffold NewPic Tauri workspace"
```

---

### Task 4: Implement versioned settings and the approved preset

**Files:**
- Create: `crates/newpic-core/src/settings.rs`
- Create: `crates/newpic-core/tests/settings.rs`
- Modify: `crates/newpic-core/src/lib.rs`

**Step 1: Write failing preset tests**

Test all approved values, not a subset:

```rust
#[test]
fn default_preset_matches_approved_design() {
    let value = Settings::default();
    assert_eq!(value.schema_version, 1);
    assert!(value.autostart);
    assert!(value.save.enabled);
    assert_eq!(value.save.directory, SaveDirectory::DownloadsNewPic);
    assert_eq!(value.save.format, ImageFormat::Png);
    assert_eq!(value.naming.pattern, "%c");
    assert_eq!(value.naming.minimum_width, 3);
    assert!(value.clipboard.enabled);
    assert_eq!(value.fixed_region, Size::new(800, 600));
    assert_eq!(value.hotkeys.region, "F7");
    assert_eq!(value.hotkeys.repeat_region, "F8");
    assert_eq!(value.hotkeys.fixed_region, "F9");
    assert_eq!(value.hotkeys.active_window, "F10");
}
```

Add tests for invalid sizes, empty shortcuts, unsupported filename tokens, corrupted JSON backup naming, and migration from an explicit schema-0 fixture.

**Step 2: Run the tests**

```bash
cargo test -p newpic-core --test settings
```

Expected: FAIL with unresolved settings types.

**Step 3: Implement the settings domain**

Use serializable value objects and a `SettingsRepository` trait. Validation returns typed errors; it never silently accepts a zero-sized region or empty save pattern. Add `load_or_recover` that returns both settings and an optional recovery notice.

**Step 4: Verify and commit**

```bash
cargo test -p newpic-core --test settings
cargo test -p newpic-core
git add crates/newpic-core
git commit -m "feat: add approved NewPic preset and settings migration"
```

---

### Task 5: Implement collision-free naming and atomic storage

**Files:**
- Create: `crates/newpic-core/src/naming.rs`
- Create: `crates/newpic-core/src/storage.rs`
- Create: `crates/newpic-core/tests/naming.rs`
- Create: `crates/newpic-core/tests/storage.rs`
- Modify: `crates/newpic-core/src/lib.rs`

**Step 1: Write failing naming tests**

Cover `001.png`, `999.png` → `1000.png`, gaps, concurrent reservations, invalid path characters, optional date/time tokens, and an existing target that must never be overwritten.

```rust
#[test]
fn sequence_expands_beyond_minimum_width() {
    assert_eq!(render_sequence(1000, 3), "1000");
}
```

**Step 2: Write failing storage tests**

Use `tempfile` and assert directory creation, sibling temporary file cleanup, atomic rename, recovery-directory fallback, and preservation of an existing file's bytes.

**Step 3: Run failures**

```bash
cargo test -p newpic-core --test naming --test storage
```

Expected: FAIL because naming and storage are unimplemented.

**Step 4: Implement minimally**

`NameAllocator` parses only documented tokens and reserves filenames with exclusive creation. `AtomicImageStore` writes to a temporary sibling, syncs, renames, and returns a typed `StoredImage` or `RecoveredImage` result.

**Step 5: Verify and commit**

```bash
cargo test -p newpic-core --test naming --test storage
git add crates/newpic-core
git commit -m "feat: add safe screenshot naming and atomic storage"
```

---

### Task 6: Implement capture geometry and provider contracts

**Files:**
- Create: `crates/newpic-platform/src/capture.rs`
- Create: `crates/newpic-platform/src/geometry.rs`
- Create: `crates/newpic-platform/src/capabilities.rs`
- Create: `crates/newpic-platform/tests/geometry.rs`
- Modify: `crates/newpic-platform/src/lib.rs`
- Modify: `crates/newpic-testing/src/lib.rs`

**Step 1: Write failing geometry tests**

Cover negative monitor origins, mixed scale factors, clamping 800×600 to a smaller monitor, invalid F8 geometry after display removal, and region intersection across adjacent monitors.

**Step 2: Define the contract**

The key types are:

```rust
pub trait CaptureProvider: Send + Sync {
    fn capabilities(&self) -> CaptureCapabilities;
    fn displays(&self) -> Result<Vec<Display>, CaptureError>;
    fn capture(&self, request: CaptureRequest) -> Result<CapturedImage, CaptureError>;
}

pub enum CaptureRequest {
    Region(PhysicalRect),
    ActiveWindow,
    Display(DisplayId),
}
```

`CapturedImage` owns RGBA pixels, width, height, row stride, scale metadata, and provenance. Reject mismatched byte lengths at construction.

**Step 3: Add a deterministic fake**

`newpic-testing` provides `FakeCaptureProvider` that records requests and returns a known 2×2 pixel fixture without mocking internal implementation calls.

**Step 4: Verify and commit**

```bash
cargo test -p newpic-platform -p newpic-testing
git add crates/newpic-platform crates/newpic-testing
git commit -m "feat: define capture contracts and monitor geometry"
```

---

### Task 7: Implement platform capture adapters with explicit capabilities

**Files:**
- Modify: `crates/newpic-windows/Cargo.toml`
- Modify: `crates/newpic-windows/src/lib.rs`
- Create: `crates/newpic-windows/tests/contract.rs`
- Modify: `crates/newpic-macos/Cargo.toml`
- Modify: `crates/newpic-macos/src/lib.rs`
- Create: `crates/newpic-macos/tests/contract.rs`
- Modify: `crates/newpic-linux/Cargo.toml`
- Modify: `crates/newpic-linux/src/lib.rs`
- Create: `crates/newpic-linux/tests/contract.rs`

**Step 1: Write platform contract tests**

Each Windows, macOS, and Linux X11 interactive test must assert that the adapter returns at least one reusable display identity, produces valid RGBA dimensions, and reports its backend and limitations. Wayland is different by design: the XDG Screenshot portal owns source selection, so `displays()` may return the typed `PortalOwnedSelection` result instead of fabricating reusable display identities. Its ignored interactive test must request a portal-owned region, validate the returned RGBA image, and verify portal-owned provenance. Mark all interactive capture tests ignored by default with a documented command to opt in; keep pure conversion and capability tests always enabled.

**Step 2: Run the local failing test**

```bash
cargo test -p newpic-linux
```

Expected: FAIL because the Linux adapter is still empty.

**Step 3: Implement adapters using `xcap` 0.9.8**

- Windows enables `xcap`'s WGC feature where supported and reports fallback use.
- macOS maps ScreenCaptureKit permission errors to `PermissionDenied` with a settings deep-link hint.
- Linux reports X11 or Wayland from the active session; Wayland cancellation is a user cancellation, not an application error. Wayland must not claim exact repeat-region or specific-display support when the portal does not return reusable geometry or display identity.
- All adapters convert pixels through one reviewed BGRA/RGBA conversion helper and never expose `xcap` types outside the adapter crate.

Active-window lookup must choose an explicitly focused, non-minimized window and reject NewPic's own overlay/settings windows. If the backend cannot distinguish content bounds, return complete window bounds plus `CaptureFallback::WholeWindow`.

**Step 4: Verify native and compile-gated code**

Run locally:

```bash
cargo test -p newpic-linux
cargo check --workspace
```

Windows CI runs `cargo test -p newpic-windows`; macOS CI runs `cargo test -p newpic-macos`. Do not treat `cargo check` on Linux as their runtime verification.

**Step 5: Commit**

```bash
git add crates/newpic-windows crates/newpic-macos crates/newpic-linux
git commit -m "feat: add native screenshot providers"
```

---

### Task 8: Implement image encoding, clipboard, and partial-success delivery

**Files:**
- Create: `crates/newpic-core/src/image_output.rs`
- Create: `crates/newpic-core/src/pipeline.rs`
- Create: `crates/newpic-core/tests/pipeline.rs`
- Create: `crates/newpic-platform/src/clipboard.rs`
- Modify: `crates/newpic-platform/src/lib.rs`
- Modify: `apps/newpic/src-tauri/Cargo.toml`

**Step 1: Write failing pipeline tests**

Test these outcomes with fakes:

- storage success + clipboard success → complete success
- storage success + clipboard failure → partial success with stored path
- storage failure + clipboard success → partial success with clipboard retained
- both fail → aggregate failure
- clipboard retries are bounded
- PNG decode matches source pixel dimensions and color values

**Step 2: Run failures**

```bash
cargo test -p newpic-core --test pipeline
```

Expected: FAIL because the pipeline does not exist.

**Step 3: Implement output ports**

Add `ImageStore` and `ImageClipboard` traits. Use `image` for PNG/JPEG encoding and an `arboard` adapter with owned RGBA bytes. Enable `arboard/wayland-data-control` for Linux only after its native build dependency check passes.

`CapturePipeline::deliver` returns a `DeliveryReport` containing independent storage and clipboard outcomes. Never return early after the first failure.

**Step 4: Verify and commit**

```bash
cargo test -p newpic-core --test pipeline
cargo test --workspace
git add crates apps/newpic/src-tauri/Cargo.toml Cargo.lock
git commit -m "feat: deliver captures to files and clipboard"
```

---

### Task 9: Wire Tauri plugins, tray, hotkeys, autostart, and commands

**Files:**
- Create: `apps/newpic/src-tauri/src/app_state.rs`
- Create: `apps/newpic/src-tauri/src/commands.rs`
- Create: `apps/newpic/src-tauri/src/hotkeys.rs`
- Create: `apps/newpic/src-tauri/src/tray.rs`
- Create: `apps/newpic/src-tauri/src/autostart.rs`
- Create: `apps/newpic/src-tauri/src/diagnostics.rs`
- Create: `apps/newpic/src-tauri/tests/hotkey_mapping.rs`
- Modify: `apps/newpic/src-tauri/src/lib.rs`
- Modify: `apps/newpic/src-tauri/capabilities/default.json`
- Modify: `apps/newpic/src-tauri/tauri.conf.json`

**Step 1: Write failing hotkey mapping tests**

Assert the exact approved mapping, per-action registration status, replacement rollback when a new shortcut conflicts, and preservation of the other three shortcuts after one failure.

**Step 2: Initialize only required plugins**

Register official global-shortcut 2.4.1, autostart 2.7.0, and notification 2.7.0 plugins. Grant the smallest capability set needed by the settings window. Do not enable shell execution or arbitrary filesystem scopes.

**Step 3: Implement tray and commands**

Tray entries: 영역 캡처, 마지막 영역 다시 캡처, 저장 영역 이동 캡처, 활성 창 캡처, 설정, NewPic 종료.

Expose typed commands for reading/saving settings, triggering captures, querying diagnostics, applying the default preset, choosing the save folder, and opening the OS permission settings where supported.

Autostart is reconciled after settings load. A failure updates diagnostics and notification state without terminating the app.

**Step 4: Verify and commit**

```bash
cargo test -p newpic-desktop --test hotkey_mapping
cargo check -p newpic-desktop
git add apps/newpic/src-tauri
git commit -m "feat: wire NewPic tray shortcuts and startup"
```

---

### Task 10: Build region and reusable F9 selection overlays

**Files:**
- Create: `apps/newpic/overlay.html`
- Create: `apps/newpic/src/overlay/main.tsx`
- Create: `apps/newpic/src/overlay/Overlay.tsx`
- Create: `apps/newpic/src/overlay/selection.ts`
- Create: `apps/newpic/src/overlay/selection.test.ts`
- Create: `apps/newpic/src/overlay/overlay.css`
- Create: `apps/newpic/src-tauri/src/overlay.rs`
- Create: `apps/newpic/src-tauri/tests/overlay_state.rs`
- Modify: `apps/newpic/vite.config.ts`
- Modify: `apps/newpic/src-tauri/src/lib.rs`

**Step 1: Write failing reducer tests**

Pure selection tests cover F7 pointer down/move/up, reverse-direction drags, and Escape cancellation without changing the F8 last-region slot. F9 tests cover first-use 800×600 centering, initialization from a saved rectangle, pointer movement without an active drag, movement with the original pointer offset, all four edge and four corner resize handles, minimum 1×1 size, display-edge clamping, keyboard movement and resizing, pointer-up without completion, Enter-only confirmation, Escape preservation, inactive/loading overlays, and physical/logical coordinate conversion with negative display origins. Coordinator tests cover atomic F9 persistence, restart restoration, display-topology fallback, stale-session rejection, and F8/F9 state separation.

**Step 2: Run failures**

```bash
npm test --workspace=@newpic/desktop -- --run src/overlay/selection.test.ts
cargo test -p newpic-desktop --test overlay_state
```

Expected: FAIL because the selection state and Rust overlay coordinator are absent.

**Step 3: Implement the overlay**

Create one borderless transparent always-on-top overlay per display. The overlay renders a dim layer, clear selected rectangle, always-visible dimension label, and no decorative motion. F7 keeps its drag-to-select behavior. F9 restores its independently saved position and size, or on first use centers an 800×600 rectangle on the primary display. Dragging inside moves the F9 rectangle; dragging any of eight named edge and corner handles resizes it. Pointer movement, ordinary clicks, and pointer-up never capture F9. Enter confirms F9, closes every overlay before taking pixels so NewPic never captures itself, and atomically persists the confirmed physical position, size, display identity, and scale metadata. Escape closes the overlay without replacing the previously saved F9 rectangle.

Keep F8 and F9 state independent. Resolve a saved F9 rectangle exactly when its display fingerprint still matches. If the saved display is missing or its resolution, orientation, or scale has changed, preserve the saved size where possible and clamp or center it inside the current primary display. Convert display-local logical coordinates to global physical coordinates using both scale and the display physical origin.

Serialize concurrent capture requests: while selection is active, subsequent hotkeys produce one “capture already active” notification instead of another overlay.

**Step 4: Verify and commit**

```bash
npm test --workspace=@newpic/desktop -- --run src/overlay/selection.test.ts
cargo test -p newpic-desktop --test overlay_state
cargo test -p newpic-desktop --test overlay_windowing
npm run typecheck
git add apps/newpic
git commit -m "feat: add region and reusable F9 capture overlays"
```

---

### Task 11: Build the settings and diagnostics UI

**Files:**
- Create: `apps/newpic/src/api/newpic.ts`
- Create: `apps/newpic/src/components/Sidebar.tsx`
- Create: `apps/newpic/src/components/SettingRow.tsx`
- Create: `apps/newpic/src/components/ShortcutField.tsx`
- Create: `apps/newpic/src/components/StatusBadge.tsx`
- Create: `apps/newpic/src/pages/GeneralPage.tsx`
- Create: `apps/newpic/src/pages/CapturePage.tsx`
- Create: `apps/newpic/src/pages/SavePage.tsx`
- Create: `apps/newpic/src/pages/FilenamePage.tsx`
- Create: `apps/newpic/src/pages/ShortcutsPage.tsx`
- Create: `apps/newpic/src/pages/DiagnosticsPage.tsx`
- Create: `apps/newpic/src/App.test.tsx`
- Modify: `apps/newpic/src/App.tsx`
- Modify: `apps/newpic/src/styles.css`

**Step 1: Write failing UI tests**

Testing Library tests must assert:

- every approved default is visible
- filename preview shows `001.png`
- selecting JPEG reveals quality and selecting PNG hides it
- shortcut conflicts are associated with their input
- diagnostics shows backend, permissions, hotkeys, autostart, and destination status
- preset reset asks for confirmation and does not mention deleting images
- keyboard navigation reaches every control

**Step 2: Run failures**

```bash
npm test --workspace=@newpic/desktop -- --run src/App.test.tsx
```

Expected: FAIL because pages/components do not exist.

**Step 3: Implement the UI**

Follow root `DESIGN.md` and existing Tauri command types. Use semantic HTML, native controls where appropriate, visible focus, system fonts, light/dark tokens, AA contrast, and reduced motion. Do not copy PicPick layout or visual assets.

**Step 4: Verify and commit**

```bash
npm test --workspace=@newpic/desktop -- --run
npm run typecheck
npm run build --workspace=@newpic/desktop
git add apps/newpic/src
git commit -m "feat: add NewPic settings and diagnostics UI"
```

---

### Task 12: Add CI, packaging, release evidence, and final verification

**Files:**
- Create: `.github/workflows/ci.yml`
- Create: `.github/workflows/build-installers.yml`
- Create: `docs/testing/manual-release-matrix.md`
- Create: `docs/testing/linux-smoke-test.md`
- Create: `docs/release/signing.md`
- Create: `docs/release/known-platform-limitations.md`
- Create: `scripts/verify-default-preset.sh`
- Modify: `README.md`
- Modify: `apps/newpic/src-tauri/tauri.conf.json`

**Step 1: Add a failing preset verification script**

The script invokes focused Rust and UI tests and fails if any approved default is absent from coverage.

Run:

```bash
bash scripts/verify-default-preset.sh
```

Expected before wiring: FAIL because the verification commands or fixtures are incomplete.

**Step 2: Add multi-OS CI**

`ci.yml` uses separate current stable runners for Ubuntu, Windows, and macOS. Every job runs formatting, lint/typecheck, unit tests, platform crate tests, and a Tauri build check. Linux installs Tauri/WebKit and clipboard dependencies from the official prerequisite list.

`build-installers.yml` produces unsigned development artifacts only when manually dispatched or tagged. It uploads MSI/EXE, DMG/app bundle, AppImage, and DEB when the runner supports them. It must label unsigned artifacts as development builds.

**Step 3: Document manual evidence**

The matrix records:

- clean install and first-launch preset
- F7 selection, F8 repeat, F9 saved-position-and-size move/resize with Enter persistence, and F10 active-window capture
- file and clipboard pixel equality
- mixed-DPI multi-monitor and negative origins
- permission denied then granted
- startup after login
- uninstall preserving captures
- Linux X11, GNOME Wayland, and KDE Wayland capability outcomes

The signing guide states that public Windows/macOS distribution remains blocked until the publisher provides signing and notarization credentials.

**Step 4: Run the full local gate**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm test -- --run
npm run typecheck
npm run build --workspace=@newpic/desktop
bash scripts/check-repo-contract.sh
bash scripts/verify-default-preset.sh
git status --short
```

Expected: all commands exit 0; `git status --short` lists only intentional documentation updates before the final commit.

**Step 5: Perform the Linux smoke test**

Run the Tauri application in an interactive Linux desktop session and attach evidence for F7 region capture, F8 repeat capture, F9 move and eight-direction resize, Enter-only persistence across repeated invocations and restart, F8/F9 separation, active-window capture, PNG output, clipboard paste, tray menu, diagnostics, and autostart. Include negative-origin and changed-display/scale fallback evidence where the desktop supports it. If no interactive evidence is recorded, mark the smoke test pending rather than passing it.

**Step 6: Review and commit**

Use `@requesting-code-review` against the approved design and this plan. Fix accepted findings, rerun the full gate, then commit:

```bash
git add .github docs scripts README.md apps/newpic/src-tauri/tauri.conf.json
git commit -m "ci: add cross-platform build and release gates"
```

---

## Completion report requirements

The implementation report must distinguish:

1. verified locally on Linux
2. verified by Windows CI
3. verified by macOS CI
4. manually verified on each graphical environment
5. pending because signing credentials, hardware, desktop session, or remote runner access were unavailable

It must include REA version and doctor result, commit range, test commands and fresh outputs, generated artifact paths, dependency licenses, external design reference and adaptations, known Wayland limitations, and the exact remaining requirements for public signing.

# Persistent Reusable F9 Region Implementation Plan

> **For Claude:** Use `${SUPERPOWERS_SKILLS_ROOT}/skills/collaboration/executing-plans/SKILL.md` to implement this plan task-by-task.

**Goal:** Replace NewPic's pointer-following, click-to-capture F9 behavior with an Enter-confirmed rectangle whose position, size, display identity, and scale metadata persist across captures and application restarts.

**Architecture:** Add a dedicated `SavedFixedRegion` geometry contract that is independent from the in-memory F8 `SavedRegion`. Persist it through settings schema v2, resolve it against current displays before opening the overlay, edit it with a pure TypeScript move/resize reducer, and save it atomically before starting capture. Keep F7 pointer-up completion and F8 last-region behavior unchanged.

**Tech Stack:** Rust 2021, serde/serde_json, Tauri 2, React 19, TypeScript 7, Vitest 5, Testing Library.

---

## Execution rules

- Follow repository `AGENTS.md`, the approved specification, clean-room policy, and root `DESIGN.md`.
- Use @test-driven-development for every behavior change: write one focused failing test, observe the expected RED, implement the minimum GREEN, then refactor.
- Never let F9 completion overwrite the F8 last-region slot.
- Never persist an F9 edit on pointer-up or Escape. Only a successful Enter confirmation may persist it.
- Use physical virtual-desktop coordinates in Rust persistence and display-local logical coordinates in each overlay webview.
- Do not claim Windows, macOS, Wayland, or graphical-session checks that were not executed.

### Task 1: Add a persisted single-display F9 geometry contract

**Files:**
- Modify: `crates/newpic-platform/Cargo.toml`
- Modify: `crates/newpic-platform/src/geometry.rs`
- Modify: `crates/newpic-platform/tests/geometry.rs`

**Step 1: Write failing geometry tests**

Add tests covering:

```rust
#[test]
fn saved_fixed_region_restores_exactly_on_unchanged_display() {
    let displays = vec![display("main", 0, 0, 1920, 1080, 1.0, true)];
    let rect = PhysicalRect::new(220, 140, 900, 520).unwrap();
    let saved = SavedFixedRegion::capture(rect, &displays).unwrap();
    assert_eq!(saved.resolve(&displays).unwrap(), rect);
}

#[test]
fn saved_fixed_region_moves_inside_primary_when_topology_changes() {
    let saved = SavedFixedRegion::capture(
        PhysicalRect::new(-1200, 80, 640, 480).unwrap(),
        &[display("left", -1280, 0, 1280, 1024, 1.0, false)],
    ).unwrap();
    let current = vec![display("main", 0, 0, 500, 400, 1.25, true)];
    assert_eq!(saved.resolve(&current).unwrap(), PhysicalRect::new(0, 0, 500, 400).unwrap());
}
```

Also test serialization round-trip, negative origins, changed scale/orientation, missing saved display, duplicate display IDs, rejection of a rectangle spanning multiple displays, and resizing a saved rectangle while retaining/clamping its top-left position.

**Step 2: Run the focused tests and verify RED**

```bash
cargo test -p newpic-platform --test geometry saved_fixed_region -- --nocapture
```

Expected: compilation fails because `SavedFixedRegion` does not exist.

**Step 3: Implement the geometry model**

Add `serde.workspace = true` to `newpic-platform`, derive serde for the primitive geometry values used in persistence, and introduce:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedFixedRegion {
    rect: PhysicalRect,
    display: DisplayFingerprint,
}

impl SavedFixedRegion {
    pub fn capture(rect: PhysicalRect, displays: &[Display]) -> Result<Self, GeometryError>;
    pub const fn rect(&self) -> PhysicalRect;
    pub fn resolve(&self, current: &[Display]) -> Result<PhysicalRect, GeometryError>;
}
```

`capture` must accept exactly one display that fully contains the rectangle. `resolve` returns the exact rectangle only when the saved display fingerprint still matches. Otherwise it centers the saved size within the current primary display, reducing it only when it cannot fit. Use a shared helper that clamps a rectangle without allowing zero dimensions.

Do not derive unchecked `Deserialize` directly for validated private geometry fields. Deserialize through primitive DTOs and call `DisplayId::new`, `PhysicalRect::new`, and `ScaleFactor::new`, so malformed zero sizes, invalid scales, and overflowing extents fail settings validation instead of constructing impossible values. Add `SavedFixedRegion::with_size(size, displays)` for Settings edits: preserve the stored top-left and display when valid, otherwise clamp within the resolved display.

**Step 4: Run focused and crate tests**

```bash
cargo test -p newpic-platform --test geometry
cargo test -p newpic-platform
```

Expected: all pass.

**Step 5: Commit**

```bash
git add crates/newpic-platform
git commit -m "feat: model persistent F9 capture geometry"
```

---

### Task 2: Migrate settings to schema v2 and persist F9 independently

**Files:**
- Modify: `crates/newpic-core/src/settings.rs`
- Modify: `crates/newpic-core/tests/settings.rs`
- Create: `crates/newpic-core/tests/fixtures/settings-schema-1.json`
- Modify: `scripts/verify-default-preset.sh`

**Step 1: Write failing settings tests**

Add tests proving:

- `CURRENT_SCHEMA_VERSION == 2`.
- default settings contain `saved_fixed_region: None` and initial size 800×600.
- schema v1 JSON migrates to v2 without inventing a position.
- schema v2 round-trips a negative-origin `SavedFixedRegion` without loss.
- invalid/zero geometry is rejected and follows the existing corrupt-settings recovery path.
- reapplying the default preset clears stored F9 geometry but never removes images.

Use a complete schema-1 fixture representing today's committed settings.

**Step 2: Verify RED**

```bash
cargo test -p newpic-core --test settings schema_one_migrates_to_persistent_f9_shape -- --exact
```

Expected: FAIL because schema v2 and the field are absent.

**Step 3: Implement schema v2**

Change settings to:

```rust
pub const CURRENT_SCHEMA_VERSION: u32 = 2;

pub struct Settings {
    // existing fields...
    pub fixed_region: Size,
    pub saved_fixed_region: Option<SavedFixedRegion>,
    pub hotkeys: Hotkeys,
}
```

Add an explicit `SchemaOneSettings` decoder and migrate `fixed_region` while setting `saved_fixed_region` to `None`. Never deserialize schema 1 directly into schema 2 via defaults; the migration must be visible and tested. Extend `verify-default-preset.sh` to require coverage for the new `None` default and schema-1 migration test, and continue requiring at least one actually executed Rust and UI default test.

**Step 4: Run focused tests and repository contract**

```bash
cargo test -p newpic-core --test settings
bash scripts/verify-default-preset.sh
bash scripts/check-repo-contract.sh
```

Expected: all pass.

**Step 5: Commit**

```bash
git add crates/newpic-core scripts/verify-default-preset.sh
git commit -m "feat: persist F9 region in settings schema v2"
```

---

### Task 3: Replace pointer-following F9 with a move/resize reducer

**Files:**
- Modify: `apps/newpic/src/overlay/selection.ts`
- Modify: `apps/newpic/src/overlay/selection.test.ts`
- Modify: `apps/newpic/src/overlay/Overlay.tsx`
- Modify: `apps/newpic/src/overlay/overlay.css`

**Step 1: Write reducer tests before production changes**

Replace the old click-to-complete F9 tests with focused tests for:

- initialization from a supplied rectangle and fallback centering at 800×600;
- pointer movement without a pressed interaction leaves the rectangle unchanged;
- pointer-down inside the rectangle then drag preserves the initial pointer offset;
- north, south, east, west, and all four corner handles resize correctly;
- reverse-direction resizing stops at minimum 1×1 instead of flipping handles;
- move and resize clamp to display-local logical bounds;
- pointer-up ends interaction but does not complete selection;
- `CONFIRM` completes only when a valid F9 rectangle exists;
- Escape restores no new value and keeps the previously persisted rectangle outside reducer state;
- F7 continues completing on pointer-up exactly as before.

The reducer API should make interaction intent explicit:

```ts
export type ResizeHandle = 'n'|'ne'|'e'|'se'|'s'|'sw'|'w'|'nw';
export type FixedInteraction =
  | { kind: 'move'; pointerStart: Point; rectStart: Rect }
  | { kind: 'resize'; handle: ResizeHandle; pointerStart: Point; rectStart: Rect };

export type SelectionAction =
  | { type: 'LOAD_OVERLAY'; mode: SelectionMode; displayBounds: DisplayBounds; rect?: Rect }
  | { type: 'INITIALIZE_FIXED'; rect: Rect }
  | { type: 'POINTER_DOWN'; point: Point; target?: 'rect' | ResizeHandle }
  | { type: 'POINTER_MOVE'; point: Point }
  | { type: 'POINTER_UP'; point?: Point }
  | { type: 'CONFIRM' }
  | { type: 'CANCEL' };
```

**Step 2: Verify RED**

```bash
npm test --workspace=@newpic/desktop -- --run src/overlay/selection.test.ts
```

Expected: old pointer-follow/click behavior fails the new assertions.

**Step 3: Implement minimal reducer behavior**

Use `LOAD_OVERLAY` after the Tauri request returns so reducer mode, display bounds, scale factor, and initial rectangle cannot remain at their pre-request defaults. Keep F7 branches unchanged. For F9, store `interaction` separately, calculate movement from `rectStart`, and calculate each resize edge from the same immutable start rectangle. Clamp before returning the next state. Only `CONFIRM` sets `status: 'completed'`.

**Step 4: Wire the overlay UI**

- Initialize F9 from `OverlayInfo.initial_rect`; otherwise center `fixed_size` in the current display.
- Render one move surface plus eight semantic resize handles with `data-handle` and appropriate CSS cursors.
- Use pointer capture so a fast drag does not lose the interaction.
- Add Enter handling that dispatches `CONFIRM`; keep Escape cancellation.
- Change the instruction to: `드래그하여 이동 · 모서리/테두리로 크기 조절 · Enter 캡처 · Esc 취소`.
- Do not invoke `complete_selection` until reducer status becomes completed through Enter.
- Convert between display-local logical coordinates and global physical coordinates by applying both scale and the display physical origin; do not merely multiply by scale.

**Step 5: Run UI tests and typecheck**

```bash
npm test --workspace=@newpic/desktop -- --run src/overlay/selection.test.ts
npm test --workspace=@newpic/desktop -- --run
npm run typecheck
```

Expected: all pass.

**Step 6: Commit**

```bash
git add apps/newpic/src/overlay
git commit -m "feat: make F9 region movable and resizable"
```

---

### Task 4: Restore and atomically save F9 through Tauri commands

**Files:**
- Modify: `apps/newpic/src-tauri/src/app_state.rs`
- Modify: `apps/newpic/src-tauri/src/overlay.rs`
- Modify: `apps/newpic/src-tauri/src/commands.rs`
- Modify: `apps/newpic/src-tauri/tests/overlay_state.rs`
- Modify: `apps/newpic/src-tauri/tests/hotkey_mapping.rs`

**Step 1: Write failing coordinator and command-level tests**

Tests must prove:

- F7 completion updates only F8's in-memory last region.
- F9 completion never changes the F8 slot.
- an F9 request contains the resolved saved rectangle after restart.
- missing/changed display topology falls back to a primary-display-clamped rectangle.
- Escape leaves both persisted F9 and F8 unchanged.
- a repository save failure prevents overlay completion, window closing, and capture dispatch.
- successful Enter persists geometry before capture and updates `fixed_region` width/height.
- saving changed width/height from Settings calls `SavedFixedRegion::with_size`, preserves the stored top-left where valid, and clamps it without changing F8.
- each overlay window receives its own display bounds rather than always using the primary display.

**Step 2: Verify RED**

```bash
cargo test -p newpic-desktop --test overlay_state -- --nocapture
```

Expected: FAIL on action separation and persistence assertions.

**Step 3: Make overlay completion action-aware**

Change the coordinator API from:

```rust
pub fn complete(&mut self, rect: PhysicalRect) -> Result<(), OverlayError>
```

to an action-aware result that returns the active request and updates `last_region` only for `Region` (including the F8 fallback selection). FixedRegion completion clears the active request without touching F8.

**Step 4: Resolve F9 before opening overlays**

When planning `CaptureAction::FixedRegion`, query displays and resolve `settings.saved_fixed_region`. Put the resulting global physical rectangle into `SelectionRequest.initial_rect`. When absent, derive an 800×600 (or current Settings size) rectangle centered and clamped in the primary display.

Update `get_overlay_info` to accept the calling `WebviewWindow`, map its `overlay-N` label to the corresponding display, and return:

```rust
pub struct OverlayInfo {
    pub request: SelectionRequest,
    pub display_bounds: PhysicalRectDto,
    pub scale_factor: f64,
    pub initial_rect: Option<PhysicalRectDto>,
}
```

Only the overlay whose display fully contains `initial_rect` receives it.

**Step 5: Persist before capture**

In `complete_selection`, inspect the active request before clearing it. For F9:

1. Validate the physical rectangle against current displays with `SavedFixedRegion::capture`.
2. Clone settings, update `fixed_region` and `saved_fixed_region`.
3. Save through the existing atomic repository.
4. Replace in-memory settings only after save succeeds.
5. Complete/close overlays and dispatch capture.

On any validation or save error, return `CommandError`, keep the selection active, and do not capture.

In the existing `persist_settings` path, compare requested `fixed_region` dimensions with the previous values. If they changed and `saved_fixed_region` exists, rebuild it with `with_size` against current displays before writing settings. If display discovery fails, return a typed settings error rather than silently storing mismatched size and geometry.

**Step 6: Run Rust tests**

```bash
cargo test -p newpic-desktop --test overlay_state
cargo test -p newpic-desktop --test hotkey_mapping
cargo test -p newpic-desktop
```

Expected: all pass.

**Step 7: Commit**

```bash
git add apps/newpic/src-tauri
git commit -m "feat: restore and persist reusable F9 regions"
```

---

### Task 5: Update settings copy, documentation, release page, and verify

**Files:**
- Modify: `apps/newpic/src/pages/CapturePage.tsx`
- Modify: `apps/newpic/src/pages/ShortcutsPage.tsx`
- Modify: `apps/newpic/src/App.test.tsx`
- Modify: `docs/plans/2026-10-22-newpic-mvp-implementation.md`
- Modify: `docs/testing/manual-release-matrix.md`
- Modify after NewPic merge: `/data/mysite/newpic.php`

**Step 1: Write failing UI copy tests**

Assert that Capture Settings describes the values as the saved F9 size, the shortcut label is `저장 영역 이동 캡처`, and no user-facing copy claims that F9 always follows the pointer or captures on click. Keep the 800×600 default assertions as first-use defaults.

**Step 2: Verify RED**

```bash
npm test --workspace=@newpic/desktop -- --run src/App.test.tsx
```

Expected: FAIL on the new labels and descriptions.

**Step 3: Update UI and documentation**

- Explain that Settings width/height changes the saved F9 size and that Enter captures/saves position and size.
- Replace the old Task 10 fixed-pointer wording in the implementation plan with the approved persistent behavior and new test matrix.
- Extend the manual release matrix for restart restoration, negative origins, display removal, changed scale, eight resize directions, and F8/F9 separation.
- After the NewPic branch is merged and artifacts are rebuilt, update `/data/mysite/newpic.php` from `F9 800×600` to `F9 저장 영역` and describe move/resize/Enter behavior. Commit that parent-site change separately in `/data/mysite` so unrelated working-tree changes are not included.

**Step 4: Run the full local gate**

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
git diff --check
git status --short
```

Expected: every command exits 0; only intentional Task 5 files remain before commit.

**Step 5: Perform available manual checks**

In an interactive Linux desktop, verify move, eight resize handles, Enter-only capture, repeat F9, restart restoration, Escape preservation, and F8 separation. If no graphical session is available, record these as pending rather than passing. Windows/macOS/manual Wayland evidence remains pending until those environments run.

**Step 6: Request code review**

Use @requesting-code-review against commit `3cc2c52`, the approved specification, and this plan. Fix all Critical/Important findings and rerun the full local gate.

**Step 7: Commit**

```bash
git add apps/newpic/src docs scripts
git commit -m "docs: align NewPic with reusable F9 capture"
```

---

## Completion report

Report the commit range, exact test commands and counts, settings migration behavior, F8/F9 separation evidence, generated Linux artifact hashes if rebuilt, and every manual/platform check that remains pending. Do not publish replacement installers until the changed F9 build passes the relevant CI and manual release matrix.

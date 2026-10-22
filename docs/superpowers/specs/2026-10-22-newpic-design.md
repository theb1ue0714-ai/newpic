# NewPic Cross-Platform Screenshot Application Design

**Status:** Approved design  
**Date:** 2026-10-22  
**Revised:** 2026-10-23 — persistent movable and resizable F9 region
**Working name:** NewPic  
**Target repository:** `/data/mysite/rea`

## 1. Purpose

NewPic is a clean-room, cross-platform desktop screenshot application for Windows, macOS, and Linux. Its first release reproduces the capture workflow and installation-time preset described in the supplied PicPick training material without copying PicPick source code, branding, icons, text, or proprietary assets.

The product will be delivered in stages. Phase 1 provides reliable capture, automatic saving, clipboard output, global shortcuts, and login startup. Later phases add image editing and utility tools without replacing the Phase 1 capture pipeline.

## 2. Confirmed Product Decisions

- Technology: Tauri 2 desktop shell with a Rust native backend and a TypeScript settings UI.
- Delivery strategy: build and stabilize the capture MVP first, then add editing and extended tools.
- Default save folder: the current user's `Downloads/NewPic` directory on every supported operating system.
- Default file format: PNG.
- Default filename pattern: `%c`, rendered with at least three digits (`001.png`, `002.png`, and so on).
- Automatic save: enabled by default.
- Copy image to clipboard: enabled by default and performed in addition to file saving.
- Login startup: enabled by default.
- Update checks: disabled by default.
- The application normally runs in the system tray without keeping a main window open.

## 3. Phase 1 Scope

### 3.1 Capture shortcuts

- **F7 — Region capture:** display a translucent selection overlay over the virtual desktop. Dragging selects the capture rectangle. Escape cancels without writing a file or changing the stored last region.
- **F8 — Repeat last region:** immediately capture the most recently completed region. If no valid region exists, or the saved rectangle no longer intersects an attached display, open the F7 selection overlay instead.
- **F9 — Reusable positioned region:** display the most recently confirmed F9 rectangle at its saved physical position and size. On first use, show an 800×600 rectangle centered on the primary display. Dragging inside the rectangle moves it; dragging any edge or corner resizes it. Pointer movement and ordinary clicks never capture. Enter captures the displayed rectangle and atomically persists its position, size, display topology, and scale metadata for the next F9 invocation and future application launches. Escape cancels without writing a file or changing the previously saved F9 rectangle.
- **F10 — Active window:** capture the content area of the foreground window when the platform exposes it. If content-only bounds are unavailable, capture the complete window bounds and notify the user that the fallback was used.

All four shortcuts are configurable. A failed or conflicting registration disables only the affected shortcut and exposes the failure in Diagnostics.

### 3.2 Capture output pipeline

After a successful capture, NewPic sends one immutable image result to independent storage and clipboard outputs:

1. Create `Downloads/NewPic` when it does not exist.
2. Reserve the next available sequence number without overwriting an existing file.
3. Encode and atomically save the image as PNG.
4. Write the same image to the operating system clipboard.
5. Show a native success or partial-failure notification.

A failure in one output must not discard a successful result from the other output. When the destination is not writable, NewPic preserves the image in a per-user recovery directory and tells the user where it was written.

### 3.3 Default preset

The first launch creates and activates this preset:

- run at login: on
- tray background mode: on
- automatic file save: on
- destination: `Downloads/NewPic`
- format: PNG
- filename: `%c`, minimum width three
- copy to clipboard: on
- open editor after capture: off
- capture success notification: on
- include pointer: off
- automatic update checking: off
- initial F9 region: 800×600 centered on the primary display until the user confirms another position and size
- shortcuts: F7, F8, F9, and F10 as defined above

The user can change every value. “Reapply default preset” restores the listed values after confirmation but never deletes captured images.

## 4. Architecture

### 4.1 Application layers

- **NewPic UI:** Tauri webview containing the Korean-first settings and diagnostics screens.
- **Tray controller:** creates the tray menu, opens settings, exposes each capture action, and exits the application.
- **Hotkey manager:** validates, registers, unregisters, and reports conflicts for global shortcuts.
- **Capture coordinator:** accepts capture commands and owns cancellation, concurrency, and result dispatch.
- **Selection overlay:** implements free-region selection and the movable, eight-direction resizable F9 rectangle across the virtual desktop. It confirms F9 only on Enter and exposes an always-visible dimensions label.
- **Capture provider:** an interface implemented separately for Windows, macOS, Linux X11, and Linux Wayland.
- **Image pipeline:** represents a capture once and supplies encoders, storage, clipboard, and future editor consumers.
- **Storage and naming:** resolves platform directories, parses filename tokens, reserves collision-free names, and performs atomic writes.
- **Clipboard adapter:** writes native image data and performs bounded retries when the clipboard is temporarily busy.
- **Autostart manager:** installs, checks, and removes per-user login startup entries.
- **Configuration service:** validates versioned settings, migrates older settings, and restores corrupted configuration safely.
- **Diagnostics service:** reports permissions, hotkey registration, autostart state, destination writability, platform backend, and recent failures.

### 4.2 Stable boundaries

`CaptureProvider` returns platform-neutral image pixels, dimensions, scale metadata, and capture provenance. The output pipeline and future editor depend only on this contract. Platform implementations must not contain naming, storage, clipboard policy, or settings UI logic.

The image pipeline accepts additional consumers so the Phase 2 editor can be added without changing capture providers.

### 4.3 Repository layout

```text
/data/mysite/rea/
  apps/newpic/                 Tauri shell and TypeScript UI
  crates/newpic-core/          Capture orchestration, settings, naming
  crates/newpic-platform/      Shared platform contracts
  crates/newpic-windows/       Windows adapter
  crates/newpic-macos/         macOS adapter
  crates/newpic-linux/         X11 and Wayland adapters
  crates/newpic-testing/       Test fixtures and platform test helpers
  research/                    REA evidence and clean-room notes
  docs/                        Product, architecture, testing, release docs
  .github/workflows/           Windows, macOS, and Linux CI
```

## 5. Platform Strategy

### 5.1 Windows

- Target Windows 10 and Windows 11 on x64 initially.
- Prefer Windows Graphics Capture where supported; use a documented fallback only when necessary.
- Use Win32 APIs for foreground-window discovery and coordinate conversion.
- Register per-user login startup.
- Produce MSI and EXE installer workflows.

### 5.2 macOS

- Target macOS 13 or later on Apple Silicon and Intel.
- Use ScreenCaptureKit for screen and window capture.
- Request Screen Recording permission only when capture is invoked or during explicit onboarding.
- Use Accessibility permission only for operations that require it, such as reliable foreground-window inspection or global input behavior.
- Explain that macOS media-key configuration may require `Fn` or enabling standard function keys for bare F7–F10 shortcuts.
- Produce DMG artifacts for both architectures, with universal packaging considered after both native builds pass.

### 5.3 Linux

- Target Ubuntu 22.04 and 24.04 families first.
- Use a native X11 implementation under X11 sessions.
- Use XDG Desktop Portal under Wayland for user-authorized capture.
- Use the desktop global-shortcut portal when the compositor provides it. If it is unavailable, show the shortcut as limited and preserve tray-triggered capture plus configurable alternatives.
- Register login startup through XDG Autostart.
- Produce AppImage and DEB artifacts first.

Wayland security intentionally prevents an application from promising identical unattended capture behavior on every compositor. NewPic reports the active backend and its capability rather than silently failing or overstating support.

## 6. Settings and User Experience

The settings window contains these sections:

1. **General:** login startup, tray behavior, notifications, and language.
2. **Capture:** pointer inclusion, the saved F9 dimensions, and multi-monitor behavior. Dimensions entered in Settings update the saved F9 size while retaining a valid stored position where possible.
3. **Save:** destination, PNG/JPEG, JPEG quality, automatic save, and clipboard output.
4. **Filename:** `%c` sequence token, optional date/time and machine tokens, minimum sequence width, and a live preview.
5. **Shortcuts:** per-action shortcut editing with conflict and registration status.
6. **Permissions & Diagnostics:** capture permission, global shortcut capability, autostart status, destination writability, backend name, and preset reset.

The visual treatment should be compact, calm, and utility-oriented. It must use native system fonts, support light and dark modes, avoid decorative animation, and honor `prefers-reduced-motion`. Before implementation, the project will adopt a sourced `DESIGN.md` following `/home/mint/AGENT.md`, and the project `AGENTS.md` will reference it.

## 7. Coordinate and Multi-Monitor Rules

- Store selection geometry in physical-pixel-aware virtual-desktop coordinates with enough metadata to translate across scale factors.
- Normalize negative monitor origins used by left- or above-positioned displays.
- The overlay spans all attached displays without adding its own pixels to the captured result.
- Revalidate stored F8 geometry after display, resolution, orientation, or scaling changes.
- Store F9 independently from F8 as a versioned physical rectangle plus the display identifiers, bounds, orientations, and scale factors needed to validate it after restart.
- When the saved F9 topology still matches, restore the exact physical rectangle, including negative virtual-desktop origins.
- When a saved display is removed or its resolution, orientation, or scale makes the rectangle invalid, preserve the saved size where it fits and move the rectangle inside the primary display. If the saved size cannot fit, reduce it to the primary display and show the actual dimensions before confirmation.
- Keep F9 inside one selected display while moving or resizing it. Clamp all eight resize directions to that display and never allow zero width or height.
- Persist the displayed F9 rectangle only after Enter is accepted. Escape, overlay creation failure, display loss during interaction, and capture cancellation retain the previously saved rectangle.

## 8. Error Handling and Recovery

- Recreate a missing destination directory.
- Never overwrite an existing capture unless the user explicitly selects a future overwrite policy.
- Write files atomically through a temporary sibling and rename.
- Bound clipboard retry time so a locked clipboard cannot block the capture service indefinitely.
- Cancel and notify if a display disappears during selection.
- Back up an unreadable settings file before restoring the default preset.
- Keep a bounded, privacy-conscious local diagnostic log without image pixels or clipboard contents.
- On a crash, offer the recent diagnostic log at the next launch.
- Uninstalling NewPic must not remove saved screenshots.

## 9. Testing and Acceptance

### 9.1 Automated tests

- Unit tests for token parsing, number allocation, collision handling, atomic-write recovery, settings validation, migration, and geometry conversion.
- Contract tests run against fake `CaptureProvider`, clipboard, storage, hotkey, and autostart adapters.
- Platform integration tests cover shortcut lifecycle, clipboard fidelity, autostart registration, PNG dimensions, and permission-state reporting.
- UI tests cover default values, validation messages, live filename preview, diagnostics, and preset reapplication.
- Reducer tests cover F9 movement with pointer offset, all four edges and four corners, minimum size, display-edge clamping, Enter-only confirmation, and Escape restoration.
- Settings and coordinator tests cover atomic F9 geometry persistence, restart restoration, schema migration, F8/F9 state separation, and monitor-topology fallback.
- CI builds and tests on Windows, macOS, and Ubuntu.

### 9.2 Manual release matrix

- Windows 10 and 11.
- macOS 13 or later on Apple Silicon and Intel.
- Ubuntu 22.04/24.04 on X11, GNOME Wayland, and KDE Wayland where available.
- One monitor, mixed-DPI multi-monitor, negative monitor origins, and monitor hot-plug.
- F9 first-use resize, repeated same-position capture, move-only repetition, resize-and-replace, restart restoration, and Escape preservation.
- Clean install, upgrade, login startup, permissions denied then granted, and uninstall with captures retained.

### 9.3 Phase 1 acceptance criteria

Phase 1 is complete when supported test systems can install NewPic, receive the approved preset, use F7/F8/F9/F10 or clearly see a platform capability limitation, save collision-free PNG files to `Downloads/NewPic`, copy matching images to the clipboard, and launch at login. Automated tests and the manual release matrix must pass for each published artifact.

Unsigned local development builds are not equivalent to publicly distributable releases. Public Windows and macOS releases require the appropriate code-signing identities and, on macOS, notarization credentials supplied by the publisher.

## 10. REA Clean-Room Research

REA will be installed from the official `morluto/rea` source and configured for the agent. Analysis is limited to a legitimately obtained PicPick v7.6.1 installer or installation and the behavior necessary to clarify the approved NewPic requirements.

Every research note records:

- target version and cryptographic hash
- acquisition source
- observed behavior and evidence
- confirmed facts versus inference
- unresolved questions and the next safe probe

The implementation must not copy decompiled code, strings beyond functional labels, icons, branding, or other proprietary assets. Engineers implementing NewPic should rely on this approved specification and sanitized behavior notes rather than PicPick internals.

## 11. Delivery Phases

### Phase 0 — Foundation

Install and verify REA, initialize the repository, add design and contribution rules, scaffold Tauri/Rust/TypeScript, and establish multi-OS CI.

### Phase 1 — Capture MVP

Implement the architecture and acceptance criteria in this document.

### Phase 2 — Image editor

Add crop, resize, rotate, shapes, arrows, text, numbered stamps, highlighting, blur/mosaic, undo/redo, and an optional post-capture editor route.

### Phase 3 — Extended utilities

Add scrolling capture, full-screen and monitor capture, color tools, ruler/crosshair/protractor tools, screen recording with optional microphone/webcam, and sharing extensions.

Phase 2 and Phase 3 require their own approved specifications before implementation.

## 12. Explicit Non-Goals for Phase 1

- Image editing tools
- Scrolling capture
- Screen recording
- Microphone or webcam capture
- Cloud upload or account systems
- Cloning PicPick appearance, branding, or proprietary assets
- Claiming unrestricted Wayland behavior where the compositor does not expose the required portal capability

## 13. Security and Privacy

- Captures remain local unless a future, separately approved sharing feature is used.
- NewPic performs no telemetry in Phase 1.
- Logs contain operational metadata only and never captured pixels.
- Permissions are requested just in time and explained before the operating system prompt.
- Platform commands and paths are passed as structured values rather than shell-concatenated strings.
- Dependencies are pinned through lockfiles and checked for license and vulnerability issues in CI.

## 14. Licensing and Naming

The repository will contain an original NewPic codebase. Third-party dependencies and REA retain their respective licenses and notices. “NewPic” remains a working name until trademark and distribution-name checks are completed; the internal application identifier must therefore be configurable before public signing.

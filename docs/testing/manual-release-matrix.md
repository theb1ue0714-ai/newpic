# NewPic Phase 1 Manual Release Matrix

Record the artifact SHA-256, tester, date, operating-system version, desktop session, display topology, and evidence link for every run. **Pending is not passing.** Never attach captured private content.

## Required environments

- Windows 10 x64 and Windows 11 x64
- macOS 13 or later on Apple Silicon and Intel
- Ubuntu 22.04 and 24.04 on X11, GNOME Wayland, and KDE Wayland
- single display; mixed-DPI multi-display; negative left/upper origins; display hot-plug

## Checks for every publishable artifact

- [ ] Clean installation succeeds and first launch applies the approved preset.
- [ ] Upgrade from the previous supported version preserves settings and captures.
- [ ] F7 selects and captures a region; Escape cancels without changing the last region.
- [ ] F8 repeats the last valid region and falls back to selection when invalid.
- [ ] F9 shows 800×600 by default and clamps visibly at display edges.
- [ ] F10 captures the active-window content or clearly reports the documented fallback.
- [ ] Saved PNG pixels and clipboard pixels are identical.
- [ ] Sequence names do not overwrite existing files and begin with `001.png` on an empty destination.
- [ ] Permission denied is explained; capture works after permission is granted.
- [ ] NewPic starts after login and remains available from the tray.
- [ ] Diagnostics accurately reports backend, permission, hotkeys, autostart, and destination.
- [ ] Uninstall preserves all captured images.

## Evidence record

Use one row per environment in the release issue: `environment | artifact hash | result | evidence | limitations`. Windows and macOS artifacts remain development-only until signing requirements are met.

## Current evidence status

- Windows 10 x64 — **pending**; no artifact hash or manual evidence yet.
- Windows 11 x64 — **pending**; no artifact hash or manual evidence yet.
- macOS 13+ Apple Silicon — **pending**; no artifact hash or manual evidence yet.
- macOS 13+ Intel — **pending**; no artifact hash or manual evidence yet.
- Ubuntu 22.04 X11 — **pending**; no interactive-session evidence yet.
- Ubuntu 24.04 X11 — **pending**; no interactive-session evidence yet.
- Ubuntu 22.04/24.04 GNOME Wayland — **pending**; no portal evidence yet.
- Ubuntu 22.04/24.04 KDE Wayland — **pending**; no portal evidence yet.

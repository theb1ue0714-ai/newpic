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
- [ ] On first use, F9 shows an 800×600 rectangle centered on the primary display; 800×600 is only the initial default, not a permanently fixed size.
- [ ] Dragging inside the F9 rectangle moves it while preserving the pointer offset; pointer movement without a drag leaves it unchanged.
- [ ] F9 resizes correctly from all eight directions: north, northeast, east, southeast, south, southwest, west, and northwest; every direction clamps visibly at the selected display edge and never reaches zero size.
- [ ] Ordinary clicks and pointer-up do not capture F9; Enter alone captures and saves the displayed position and size.
- [ ] Repeating F9 restores the last Enter-confirmed position and size, including after NewPic exits and restarts.
- [ ] Escape cancels F9 without writing a file or replacing the previously saved position and size.
- [ ] A saved F9 rectangle on a display with a negative left or upper origin restores to the exact physical coordinates when the display topology is unchanged.
- [ ] Removing the saved display moves F9 inside the current primary display, preserving its saved size when it fits and reducing it only when required.
- [ ] Changing the saved display's scale, resolution, or orientation safely revalidates F9 on the current primary display and shows the actual dimensions before Enter.
- [ ] F8 and F9 histories remain separate: completing F9 does not replace the F8 last region, and completing F7/F8 does not replace the saved F9 rectangle.
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

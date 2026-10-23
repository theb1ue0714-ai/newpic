# Linux Interactive Smoke Test

This test requires a real graphical session; SSH-only or headless execution cannot pass it.

1. Record `uname -a`, distribution, `$XDG_SESSION_TYPE`, desktop/compositor, display layout/scales, and artifact SHA-256.
2. Install the unsigned DEB or AppImage development artifact and launch NewPic.
3. Confirm the default preset in Settings and the backend/capability report in Diagnostics.
4. Exercise F7 and Escape cancellation, then confirm F8 repeats only the last F7 region.
5. On first F9 use, confirm the 800×600 rectangle is centered on the primary display. Move it from inside, resize it from all four edges and four corners, and verify ordinary clicks and pointer-up do not capture. Press Enter and confirm the position and size are saved.
6. Invoke F9 again and after restarting NewPic to confirm the last Enter-confirmed position and size return. Press Escape after another edit and confirm the previously saved rectangle remains unchanged.
7. Confirm completing F9 does not replace the F8 region, and completing F7/F8 does not replace the saved F9 rectangle.
8. Exercise F10 active-window capture.
9. Compare PNG dimensions and pixels with a clipboard paste into an independent local application.
10. Confirm tray menu actions, notification behavior, destination recovery, and login autostart.
11. Repeat permission denial/grant, negative-origin placement, mixed scale factors, display removal, scale changes, and monitor hot-plug where the desktop supports them. Confirm F9 restores exactly when topology matches and falls back visibly inside the primary display when it does not.
12. Uninstall NewPic and verify `Downloads/NewPic` remains intact.

Run separately on X11, GNOME Wayland, and KDE Wayland. Under Wayland, record the portal chooser and global-shortcut capability exactly rather than treating compositor restrictions as product failures. Do not record screen pixels containing personal data.

**Current server status:** pending — no interactive desktop evidence has been recorded.

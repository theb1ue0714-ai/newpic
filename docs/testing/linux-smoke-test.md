# Linux Interactive Smoke Test

This test requires a real graphical session; SSH-only or headless execution cannot pass it.

1. Record `uname -a`, distribution, `$XDG_SESSION_TYPE`, desktop/compositor, display layout/scales, and artifact SHA-256.
2. Install the unsigned DEB or AppImage development artifact and launch NewPic.
3. Confirm the default preset in Settings and the backend/capability report in Diagnostics.
4. Exercise F7, Escape cancellation, F8 repeat, F9 fixed region, and F10 active window.
5. Compare PNG dimensions and pixels with a clipboard paste into an independent local application.
6. Confirm tray menu actions, notification behavior, destination recovery, and login autostart.
7. Repeat permission denial/grant and monitor hot-plug where the desktop supports them.
8. Uninstall NewPic and verify `Downloads/NewPic` remains intact.

Run separately on X11, GNOME Wayland, and KDE Wayland. Under Wayland, record the portal chooser and global-shortcut capability exactly rather than treating compositor restrictions as product failures. Do not record screen pixels containing personal data.

**Current server status:** pending — no interactive desktop evidence has been recorded.

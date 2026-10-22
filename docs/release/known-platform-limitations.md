# Known Platform Limitations

## Linux Wayland

- XDG Desktop Portal may require a user chooser for each capture and can prevent unattended repeat capture.
- Active-window targeting and pointer inclusion depend on portal/compositor support.
- Global shortcuts depend on the desktop global-shortcut portal; tray actions and configurable alternatives remain available when unsupported.
- GNOME and KDE can expose different capability results. Diagnostics is authoritative for the current session.

## Linux X11

Native capture and shortcuts are expected, but mixed-DPI coordinate behavior varies by desktop configuration and requires manual evidence.

## macOS

Screen Recording permission is requested by the operating system. Active-window discovery can also require Accessibility permission. Bare F7–F10 may require `Fn` or enabling standard function keys.

## Windows

Foreground-window APIs may expose full-window bounds when content-only bounds are unavailable; NewPic reports that fallback. Public installers require Authenticode signing.

## All platforms

The Phase 1 editor switch remains off because editing is a separately specified later phase. The `NewPic` name and `app.newpic.desktop` identifier are provisional until publisher approval and trademark review.

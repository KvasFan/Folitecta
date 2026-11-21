## Folitecta 0.113.0 — a clearer workspace, easier installation

- **A more organized desktop**: aligned Open, New and Combine actions, a distinct recent-files
  area, command-search tips, clearer selected ribbon tabs and softer document shadows.
  Start-screen actions and recent files are real keyboard-accessible buttons, with visible focus;
  reduced-motion preferences are respected. English and Italian are supported throughout.
- **Windows installer**: a setup wizard, Start menu shortcuts, PDF Open With registration and an
  uninstaller, installed for the current user. The portable ZIP remains available.
- **Linux installation**: a Debian/Ubuntu `.deb` that declares its runtime dependencies and adds
  the app to the desktop menu; the portable archive also includes a per-user `install.sh`.
- **First Android reader** (Android 8+): open PDFs from the system picker or another app, browse
  and jump between pages, zoom and share a read-only copy. Offline, in English and Italian.
  This version is a reader; editing and password-protected PDFs are not supported on Android.
- **A shorter GitHub project page**, with downloads first, a feature overview and the full desktop
  reference kept in [the desktop guide](https://github.com/KvasFan/Folitecta/blob/v0.113.0/docs/DESKTOP_GUIDE.md).

This release also includes the desktop improvements developed since 0.73.0, through the work
planned as 0.112.0: content selection and reused drawings, imported links, Times/Courier text
boxes, font repair, crop selection, annotation templates, richer link actions and more.
See [CHANGELOG.md](https://github.com/KvasFan/Folitecta/blob/v0.113.0/CHANGELOG.md) for the full list.

| Platform | Recommended asset |
| --- | --- |
| Windows 10 / 11 x64 | `folitecta-0.113.0-windows-x64-setup.exe` |
| Debian / Ubuntu x64 | `folitecta-0.113.0-linux-amd64.deb` |
| Other Linux x64 | `folitecta-0.113.0-linux-x64.tar.gz` |
| Android 8.0+ | `folitecta-0.113.0-android.apk` |

SHA-256 checksums accompany every asset. Keep `fonts`, `ocr` and `dictionaries` beside portable
desktop executables. Linux needs glibc 2.35+ and WebKitGTK 4.1; the `.deb` installs dependencies.
The Windows installer is not code-signed. The previous macOS build remains available in 0.73.0. Android builds use
a persistent project release key. This is preview software; see the
[feature matrix](https://github.com/KvasFan/Folitecta/blob/v0.113.0/docs/FEATURES.md) and
[known limitations](https://github.com/KvasFan/Folitecta/blob/v0.113.0/docs/ARCHITECTURE.md#known-limitations).

GitHub-hosted Actions were unavailable because of the account billing lock. These three platform
packages were built and checked locally, then uploaded directly. The workflow is retained for
future automated builds, including macOS.

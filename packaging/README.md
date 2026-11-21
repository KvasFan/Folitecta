# Release packages

Builds use the tested Rust 1.99.1 toolchain pinned in `rust-toolchain.toml`.
Tagging `v<workspace version>` runs `.github/workflows/release.yml`. All four platform jobs must
succeed before publication. Every asset has its own SHA-256 checksum. The first release with
installers and an Android reader is **0.113.0**.

| Platform | Packages | Verification in the release job |
| --- | --- | --- |
| Windows x64 | NSIS setup `.exe`, portable `.zip` | C runtime imports, silent install, installed version, silent uninstall |
| Linux x64 | Debian `.deb`, portable `.tar.gz` with user installer | ELF imports, dependency installation, installed version/resources, removal |
| Android 8+ | Signed universal `.apk` (native platform reader, no ABI-specific library) | Android lint, APK signature verification |
| macOS universal | `.app` in `.zip`, portable `.tar.gz` | Ad hoc bundle signature verification |

The desktop payload includes OCR models, fonts, dictionaries and their licences. The Debian
package installs into `/opt/folitecta`, with an executable link, PDF Open With menu entry and icon.
For other Linux distributions, the archive's `install.sh` installs for the current user in
`${XDG_DATA_HOME:-$HOME/.local/share}/folitecta`, with a launcher in `~/.local/bin` and a desktop
menu entry. It needs Python 3. Remove those files to uninstall the user installation.

The Windows wizard installs per user in `%LOCALAPPDATA%/Programs/Folitecta`, without elevation.
It registers **Open with Folitecta**, without changing the user's default PDF application, and
adds an uninstaller to Windows settings. Windows and macOS releases are not publisher-signed.

Android release signing requires four repository Actions secrets:
`ANDROID_KEYSTORE_BASE64`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS`, `ANDROID_KEY_PASSWORD`.
The keystore must be backed up privately and reused for future updates. An unsigned or
debug-signed APK is never substituted for a missing release key. See [the Android guide](../android/README.md).

Local installer builds:

```sh
packaging/linux/build-deb.sh dist/folitecta-0.113.0-linux-x64 0.113.0 dist/folitecta-0.113.0-linux-amd64.deb
```

On Windows, after preparing the same release payload and installing NSIS:

```powershell
makensis /DVERSION=0.113.0 /DPAYLOAD=C:\path\to\payload packaging/windows/folitecta.nsi
```

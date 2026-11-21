#!/usr/bin/env bash
# Package the release payload produced by release.yml, preserving its data folders.
set -euo pipefail
payload=$(realpath "${1:?usage: build-deb.sh PAYLOAD VERSION OUTPUT}")
version=${2:?missing version}
output=$(realpath -m "${3:?missing output}")
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo 'Expected a semantic version' >&2; exit 1; }
test -x "$payload/folitecta"
test -f "$payload/THIRD-PARTY-LICENSES.txt"
staging=$(mktemp -d)
trap 'rm -rf "$staging"' EXIT
mkdir -p "$staging/DEBIAN" "$staging/opt/folitecta" "$staging/usr/bin" \
  "$staging/usr/share/applications" "$staging/usr/share/icons/hicolor/256x256/apps" \
  "$staging/usr/share/doc/folitecta"
cp -a "$payload/." "$staging/opt/folitecta/"
chmod 755 "$staging/opt/folitecta/folitecta"
ln -s /opt/folitecta/folitecta "$staging/usr/bin/folitecta"
cp packaging/linux/folitecta.desktop "$staging/usr/share/applications/"
cp crates/folitecta/assets/icon-256.png "$staging/usr/share/icons/hicolor/256x256/apps/folitecta.png"
cp LICENSE "$staging/usr/share/doc/folitecta/copyright"
size=$(du -sk "$staging/opt" | cut -f1)
cat > "$staging/DEBIAN/control" <<CONTROL
Package: folitecta
Version: $version
Section: editors
Priority: optional
Architecture: amd64
Maintainer: KvasFan <177037241+KvasFan@users.noreply.github.com>
Installed-Size: $size
Depends: libc6 (>= 2.35), libgcc-s1, libstdc++6, libwebkit2gtk-4.1-0 (>= 2.38), libgtk-3-0 (>= 3.24), libssl3
Homepage: https://github.com/KvasFan/Folitecta
Description: Free PDF editor
 Read, annotate, sign, protect, organize and print PDF documents.
 Dioxus desktop interface, English and Italian, MIT licensed.
CONTROL
mkdir -p "$(dirname "$output")"
dpkg-deb --root-owner-group --build "$staging" "$output"
dpkg-deb --info "$output"

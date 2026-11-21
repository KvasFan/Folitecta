#!/usr/bin/env bash
# Run from an extracted Linux archive; installs only for the current user.
set -euo pipefail
source_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
data_root=${XDG_DATA_HOME:-"$HOME/.local/share"}
install_dir="$data_root/folitecta"
[[ "$source_dir" != "$install_dir" ]] || { echo 'Already installed here.'; exit 0; }
test -x "$source_dir/folitecta"
mkdir -p "$install_dir" "$HOME/.local/bin" "$data_root/applications" "$data_root/icons/hicolor/256x256/apps"
cp -a "$source_dir/." "$install_dir/"
ln -sfn "$install_dir/folitecta" "$HOME/.local/bin/folitecta"
# Quote paths for the desktop entry (including spaces and percent signs).
python3 - "$install_dir/folitecta" "$data_root/applications/folitecta.desktop" <<'PY'
import pathlib, sys
exe = sys.argv[1].replace('\\', '\\\\\\\\').replace('"', '\\\\"').replace('`', '\\\\`').replace('$', '\\\\$').replace('%', '%%')
pathlib.Path(sys.argv[2]).write_text('[Desktop Entry]\nType=Application\nName=Folitecta\nComment=Free PDF editor\nExec="' + exe + '" %F\nIcon=folitecta\nTerminal=false\nMimeType=application/pdf;\nCategories=Office;Viewer;\n')
PY
cp "$source_dir/folitecta.png" "$data_root/icons/hicolor/256x256/apps/folitecta.png"
if command -v update-desktop-database >/dev/null; then update-desktop-database "$data_root/applications"; fi
printf 'Installed Folitecta in %s\nLaunch it from your application menu.\n' "$install_dir"

#!/bin/sh
# Installation de Halcyon pour l'utilisateur courant (sans droits administrateur).
set -e
DIR="$(cd "$(dirname "$0")" && pwd)"
BIN="${XDG_BIN_HOME:-$HOME/.local/bin}"
DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
mkdir -p "$BIN" "$DATA/applications" "$DATA/icons/hicolor/256x256/apps" "$DATA/mime/packages"
install -m 755 "$DIR/halcyon" "$BIN/halcyon"
install -m 644 "$DIR/halcyon.png" "$DATA/icons/hicolor/256x256/apps/halcyon.png"
install -m 644 "$DIR/halcyon-mime.xml" "$DATA/mime/packages/halcyon.xml"
sed "s|^Exec=halcyon|Exec=$BIN/halcyon|" "$DIR/halcyon.desktop" > "$DATA/applications/halcyon.desktop"
command -v update-mime-database >/dev/null 2>&1 && update-mime-database "$DATA/mime" || true
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$DATA/applications" || true
echo "Halcyon est installé : lancez-le depuis le menu des applications ou avec $BIN/halcyon"

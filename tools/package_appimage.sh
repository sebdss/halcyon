#!/bin/sh
# AppImage de Halcyon (Linux x86-64), mise à jour par HUS.
# Usage : tools/package_appimage.sh   (depuis la racine du dépôt, après
#         cargo build --release -p halcyon-app -p halcyon-update --bin halcyon-updater)
# Sortie : dist/<version>/Halcyon-<version>-x86_64.AppImage
set -eu
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
BIN=${HALCYON_BIN:-target/release/halcyon}
UPDATER=${HALCYON_UPDATER_BIN:-target/release/halcyon-updater}
OUT=dist/$VERSION
RUNTIME=packaging/appimage/runtime-x86_64
[ -x "$BIN" ] && [ -x "$UPDATER" ] || { echo "Compilez d'abord : cargo build --release -p halcyon-app -p halcyon-update --bin halcyon-updater" >&2; exit 1; }
command -v mksquashfs >/dev/null || { echo "mksquashfs absent (paquet squashfs-tools)" >&2; exit 1; }
# Moteur versionné dans le dépôt, empreinte contrôlée.
(cd packaging/appimage && sha256sum -c runtime-x86_64.sha256 >/dev/null) || { echo "empreinte du moteur AppImage incorrecte" >&2; exit 1; }
mkdir -p "$OUT"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
A="$TMP/Halcyon.AppDir"
mkdir -p "$A/usr/bin" "$A/usr/share/doc/halcyon" "$A/usr/share/applications" "$A/usr/share/icons/hicolor/256x256/apps"
install -m 755 "$BIN" "$A/usr/bin/halcyon"
install -m 755 "$UPDATER" "$A/usr/bin/halcyon-updater"
strip "$A/usr/bin/halcyon" "$A/usr/bin/halcyon-updater" 2>/dev/null || true
install -m 755 packaging/appimage/AppRun "$A/AppRun"
install -m 644 packaging/linux/halcyon.png "$A/halcyon.png"
install -m 644 packaging/linux/halcyon.png "$A/usr/share/icons/hicolor/256x256/apps/halcyon.png"
ln -s halcyon.png "$A/.DirIcon"
# Entrée de bureau de l'AppImage (Exec relatif, exigé par le format).
sed -e 's|^Exec=.*|Exec=halcyon %f|' -e 's|^Icon=.*|Icon=halcyon|' packaging/linux/halcyon.desktop > "$A/halcyon.desktop"
cp "$A/halcyon.desktop" "$A/usr/share/applications/halcyon.desktop"
for f in README.md CHANGELOG.md docs/GUIDE.md docs/HUS.md crates/halcyon-app/ui/fonts/LICENSE-Inter.txt crates/halcyon-app/ui/fonts/LICENSE-Sora.txt packaging/appimage/README.md; do
  [ -f "$f" ] && install -m 644 "$f" "$A/usr/share/doc/halcyon/$(basename "$f")"
done
mksquashfs "$A" "$TMP/fs.sqfs" -root-owned -noappend -comp zstd -quiet -no-progress
IMG="$OUT/Halcyon-$VERSION-x86_64.AppImage"
cat "$RUNTIME" "$TMP/fs.sqfs" > "$IMG"
chmod 755 "$IMG"
ls -l "$IMG"

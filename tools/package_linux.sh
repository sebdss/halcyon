#!/bin/sh
# Paquets Linux et archive des sources, à partir d'une compilation release.
# Usage : tools/package_linux.sh   (depuis la racine du dépôt, après
#         cargo build --release -p halcyon-app -p halcyon-installer)
set -eu
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
BIN=target/release/halcyon
OUT=dist/$VERSION
SETUP=target/release/halcyon-setup
UPDATER=target/release/halcyon-updater
[ -x "$BIN" ] && [ -x "$SETUP" ] && [ -x "$UPDATER" ] || { echo "Compilez d'abord : cargo build --release -p halcyon-app -p halcyon-installer -p halcyon-update" >&2; exit 1; }
mkdir -p "$OUT"
# glibc de la machine de compilation : version minimale requise par les binaires.
GLIBC=$(getconf GNU_LIBC_VERSION | cut -d' ' -f2)
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

# --- .deb
D="$TMP/deb"
mkdir -p "$D/DEBIAN" "$D/usr/bin" "$D/usr/share/applications" "$D/usr/share/icons/hicolor/256x256/apps" \
  "$D/usr/share/mime/packages" "$D/usr/share/doc/halcyon"
install -m 755 "$BIN" "$D/usr/bin/halcyon"
strip "$D/usr/bin/halcyon" 2>/dev/null || true
install -m 644 packaging/linux/halcyon.desktop "$D/usr/share/applications/halcyon.desktop"
install -m 644 packaging/linux/halcyon.png "$D/usr/share/icons/hicolor/256x256/apps/halcyon.png"
install -m 644 packaging/linux/halcyon-mime.xml "$D/usr/share/mime/packages/halcyon.xml"
printf 'Halcyon %s\nCode : MIT. Interface : Slint. Police : Inter (SIL OFL).\n' "$VERSION" > "$D/usr/share/doc/halcyon/copyright"
SIZE=$(du -sk "$D/usr" | cut -f1)
cat > "$D/DEBIAN/control" <<EOF
Package: halcyon
Version: $VERSION-1
Architecture: amd64
Section: games
Priority: optional
Maintainer: Sebi
Installed-Size: $SIZE
Depends: libc6 (>= $GLIBC), libfontconfig1 (>= 2.12.6), libudev1 (>= 183), libasound2t64 | libasound2
Description: Halcyon — launcher universel de jeux vidéo
 Launcher universel de jeux vidéo : Steam, Epic (Heroic/Legendary), jeux
 standalone, émulateurs, succès et mode Big Picture.
EOF
dpkg-deb --root-owner-group --build "$D" "$OUT/halcyon_${VERSION}_amd64.deb" >/dev/null

# --- archive portable
A="$TMP/halcyon-$VERSION"
mkdir -p "$A"
install -m 755 "$D/usr/bin/halcyon" "$A/halcyon"
cp packaging/linux/halcyon.desktop packaging/linux/halcyon.png packaging/linux/halcyon-mime.xml \
  packaging/linux/install.sh packaging/linux/uninstall.sh "$A/"
cp README.md CHANGELOG.md docs/GUIDE.md docs/MISE-A-JOUR-0.4.md docs/SON.md "$A/"
tar -C "$TMP" -czf "$OUT/halcyon-$VERSION-linux-x86_64.tar.gz" "halcyon-$VERSION"

# --- programme d'installation (par utilisateur, sans droits administrateur)
S="$TMP/setup"
mkdir -p "$S"
install -m 755 "$D/usr/bin/halcyon" "$S/halcyon"
# Updater HUS : la première mise à jour automatique fait passer cette installation à l'AppImage.
install -m 755 "$UPDATER" "$S/halcyon-updater"
strip "$S/halcyon-updater" 2>/dev/null || true
install -m 644 packaging/linux/halcyon.png packaging/linux/halcyon-mime.xml "$S/"
install -m 644 README.md CHANGELOG.md docs/GUIDE.md docs/SON.md docs/INSTALLATEUR.md docs/HUS.md "$S/"
install -m 755 "$SETUP" "$TMP/stub"
strip "$TMP/stub" 2>/dev/null || true
"$TMP/stub" --pack "$S" "$VERSION" "$OUT/halcyon-$VERSION-linux-x86_64-setup" --min-glibc "$GLIBC"

# --- sources (commit courant)
git archive --format=zip --prefix=halcyon/ -o "$OUT/halcyon-$VERSION-source.zip" HEAD

(cd "$OUT" && sha256sum "halcyon_${VERSION}_amd64.deb" "halcyon-$VERSION-linux-x86_64-setup" "halcyon-$VERSION-linux-x86_64.tar.gz" "halcyon-$VERSION-source.zip" > SHA256SUMS)
ls -l "$OUT"

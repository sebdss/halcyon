#!/bin/sh
# Test de bout en bout de HUS sous Linux avec de vraies AppImage de Halcyon.
#
# Usage : tools/hus_e2e_linux.sh <dossier> <AppImage installée> <AppImage nouvelle> <version nouvelle> <résultat attendu : 0|2>
# Variables : HUS_UPDATER (updater compilé avec HALCYON_UPDATE_PUBKEY), HUS_RELEASE (outil
# hus-release), HUS_SK (fichier de clé privée de test), HUS_PK (clé publique de test).
# Simule ce que fait Halcyon après téléchargement (paquet vérifié mis en attente), puis lance
# l'updater comme Halcyon le fait : installation, démarrage de la nouvelle version, contrôle
# de santé, et retour arrière si elle ne démarre pas.
set -eu
DIR=$1 OLD=$2 NEW=$3 VERSION=$4 EXPECT=$5
rm -rf "$DIR" && mkdir -p "$DIR/opt" "$DIR/pub"
install -m 755 "$OLD" "$DIR/opt/Halcyon.AppImage"
PKG="Halcyon-$VERSION-x86_64.AppImage"
cp "$NEW" "$DIR/pub/$PKG"
echo "Test de bout en bout HUS." > "$DIR/notes.txt"
"$HUS_RELEASE" manifest --version "$VERSION" --notes "$DIR/notes.txt" --out "$DIR/pub/halcyon-manifest.json" "$DIR/pub/$PKG" >/dev/null
HALCYON_SIGNING_KEY=$(cat "$HUS_SK") "$HUS_RELEASE" sign "$DIR/pub/halcyon-manifest.json" >/dev/null
"$HUS_RELEASE" verify "$DIR/pub" --pubkey "$(cat "$HUS_PK")" >/dev/null
mkdir -p "$DIR/opt/.hus/pending"
cp "$DIR/pub/"* "$DIR/opt/.hus/pending/"
printf '{"version":"%s","package":"%s","target":{"kind":"appimage","file":"Halcyon.AppImage"},"staged_at":0}' "$VERSION" "$PKG" > "$DIR/opt/.hus/pending/pending.json"
printf '{"state":"Staging","to":"%s","acknowledged":true}' "$VERSION" > "$DIR/opt/.hus/state.json"
OLD_SHA=$(sha256sum "$DIR/opt/Halcyon.AppImage" | cut -d' ' -f1)
set +e
"$HUS_UPDATER" apply --root "$DIR/opt" --current 1.0.0-alpha.1 --health-timeout 120
CODE=$?
set -e
echo "--- journal"; cat "$DIR/opt/.hus/hus.log"
echo "--- état"; cat "$DIR/opt/.hus/state.json"; echo
NOW_SHA=$(sha256sum "$DIR/opt/Halcyon.AppImage" | cut -d' ' -f1)
NEW_SHA=$(sha256sum "$NEW" | cut -d' ' -f1)
[ "$CODE" = "$EXPECT" ] || { echo "ÉCHEC : code $CODE, attendu $EXPECT"; exit 1; }
if [ "$EXPECT" = 0 ]; then
  [ "$NOW_SHA" = "$NEW_SHA" ] || { echo "ÉCHEC : nouvelle AppImage absente"; exit 1; }
  [ -f "$DIR/opt/.hus/previous/1.0.0-alpha.1/Halcyon.AppImage" ] || { echo "ÉCHEC : version précédente non conservée"; exit 1; }
else
  [ "$NOW_SHA" = "$OLD_SHA" ] || { echo "ÉCHEC : ancienne AppImage non restaurée"; exit 1; }
fi
echo "RÉUSSI (code $CODE)"

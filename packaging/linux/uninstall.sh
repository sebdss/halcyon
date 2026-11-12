#!/bin/sh
# Désinstallation de Halcyon (installation utilisateur).
#   ./uninstall.sh          : retire le programme, ses raccourcis et le démarrage automatique ;
#                             les données (bibliothèque, copies de sauvegardes) sont conservées.
#   ./uninstall.sh --purge  : supprime aussi les données, après confirmation.
BIN="${XDG_BIN_HOME:-$HOME/.local/bin}"
DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
CONFIG="${XDG_CONFIG_HOME:-$HOME/.config}"
CACHE="${XDG_CACHE_HOME:-$HOME/.cache}"
rm -f "$BIN/halcyon" "$DATA/applications/halcyon.desktop" "$DATA/icons/hicolor/256x256/apps/halcyon.png" \
      "$DATA/mime/packages/halcyon.xml" "$CONFIG/autostart/halcyon.desktop"
# Raccourcis de jeux créés par Halcyon (menu des applications et Bureau).
rm -f "$DATA"/applications/halcyon-jeu-*.desktop
DESKTOP="$(command -v xdg-user-dir >/dev/null 2>&1 && xdg-user-dir DESKTOP || echo "$HOME/Desktop")"
rm -f "$DESKTOP"/halcyon-jeu-*.desktop
command -v update-mime-database >/dev/null 2>&1 && update-mime-database "$DATA/mime" || true
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$DATA/applications" || true
if [ "$1" = "--purge" ]; then
    printf "Supprimer définitivement %s (bibliothèque, paramètres, copies de sauvegardes de jeux) ? [o/N] " "$DATA/Halcyon"
    read -r answer
    case "$answer" in
        o|O|oui|y|Y) rm -rf "$DATA/Halcyon" "$CACHE/Halcyon"; echo "Données supprimées." ;;
        *) echo "Données conservées." ;;
    esac
else
    echo "Halcyon a été désinstallé. Données conservées dans $DATA/Halcyon (./uninstall.sh --purge pour les supprimer)."
fi

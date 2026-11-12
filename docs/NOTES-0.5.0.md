# Halcyon 0.5.0 — « Minecraft Integration »

La 0.5.0 ajoute Minecraft Java Edition à Halcyon, dans un nouvel onglet du menu. Rien n'est
retiré : accueil, bibliothèque, Big Picture, émulateurs, sauvegardes, son, installateur et
toutes les autres fonctions de la 0.4.1 sont inchangés, et leurs données sont reprises telles
quelles.

## Ce qui change pour vous

- **Un onglet Minecraft** avec six sections : Jouer, Versions, Mods, Modpacks, Loaders,
  Paramètres. Tout le fonctionnement est décrit dans [`MINECRAFT.md`](MINECRAFT.md).
- **Installer n'importe quelle version** publiée par Mojang, avec le bon Java téléchargé
  automatiquement (runtime officiel Mojang).
- **Fabric, Quilt, Forge, NeoForge** installés par leurs mécanismes officiels.
- **Mods et modpacks** depuis Modrinth (CurseForge avec votre clé d'API), en ne proposant que
  ce qui est compatible avec la version et le loader de l'installation choisie.
- **Jouer par le launcher officiel** : Halcyon prépare tout et ajoute l'installation au launcher
  officiel, qui connecte votre compte Microsoft et lance le jeu. Aucun identifiant ni jeton n'est
  lu ou stocké par Halcyon dans ce mode.
- **Jouer directement depuis Halcyon** quand un identifiant d'application Azure approuvé par
  Mojang est renseigné (connexion Microsoft officielle par code).

## Sécurité et respect des plateformes

- Aucun compte « crack », aucune authentification contournée, aucun mot de passe stocké.
- Le fichier `launcher_profiles.json` du launcher officiel n'est modifié qu'après votre accord,
  avec une sauvegarde préalable ; seuls les profils « Halcyon · … » sont ajoutés ou retirés.
- Téléchargements en HTTPS, vérifiés par SHA-1 (Mojang, Modrinth) ; fichiers de modpack
  limités aux hôtes autorisés et au dossier de l'installation.
- Les fichiers que les auteurs CurseForge n'autorisent pas hors de leur site ne sont pas
  téléchargés.

## Corrections incluses depuis la 0.4.1

- Jeux Epic Games de nouveau détectés.
- Illustrations SteamGridDB pour les jeux autonomes.
- Émulateurs personnalisés.

## Limites

Voir [`MINECRAFT.md`](MINECRAFT.md#limites-connues) et le rapport de tests
[`TESTS-0.5.0.md`](TESTS-0.5.0.md).

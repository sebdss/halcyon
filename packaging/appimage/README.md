# Moteur AppImage (type 2)

`runtime-x86_64` est le moteur d'exécution des AppImage de type 2
([AppImage/type2-runtime](https://github.com/AppImage/type2-runtime), licence MIT),
binaire statique placé devant l'image SquashFS de Halcyon par `tools/package_appimage.sh`.

Il est versionné dans le dépôt pour que la compilation ne télécharge rien : la publication
est reproductible et ne dépend pas d'un fichier distant modifiable. Son empreinte est
contrôlée à chaque construction (`runtime-x86_64.sha256`).

- Source : `https://github.com/AppImage/type2-runtime/releases/download/continuous/runtime-x86_64`
- Récupéré le 16 novembre 2026
- Statique : ne demande pas `libfuse2` ; il utilise `fusermount3` (paquet `fuse3`, présent par
  défaut sur les distributions récentes). Sans FUSE, Halcyon se lance avec
  `--appimage-extract-and-run` (voir `docs/HUS.md`).

Mise à jour du moteur : remplacer le fichier, régénérer l'empreinte
(`sha256sum runtime-x86_64 > runtime-x86_64.sha256`), committer les deux.

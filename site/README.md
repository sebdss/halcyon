# Site officiel de Halcyon

Site vitrine statique, sans dépendance ni étape de compilation : HTML, CSS et JavaScript
écrits à la main. Il reprend le système de design « Berge » du launcher
(`docs/DESIGN-SYSTEM.md`) : mêmes couleurs, même police Archivo, mêmes icônes, la perche
pour l'élément actif, l'orange réservé au geste principal.

```
site/
├── index.html               page d'accueil (français)
├── mentions-legales.html
├── confidentialite.html
├── en/                      version anglaise : index.html, legal-notice.html, privacy.html
├── site.webmanifest, robots.txt, sitemap.xml
└── assets/
    ├── css/site.css         jetons et composants
    ├── js/config.js         dépôt, canal, fichiers proposés  ← à modifier
    ├── js/site.js           navigation, onglets, galerie, téléchargement (textes FR et EN)
    ├── fonts/               Archivo (SIL OFL), sous-ensemble latin en woff2
    ├── video/               film de présentation : halcyon-film-{fr,en}-{1080,720}.mp4,
    │                        images d'attente halcyon-film-{fr,en}-poster.webp
    └── img/                 logo, favicons, images Open Graph (og.png, og-en.png),
                             captures (screens/ en français, screens/en/ en anglais)
```

## Langues

Deux versions, une par dossier : le français à la racine, l'anglais dans `en/`. Elles
partagent le CSS, le JavaScript, les polices et les images ; seules les captures ont une
copie anglaise (`assets/img/screens/en/`). Chaque page déclare l'autre (`hreflang`, aussi
dans `sitemap.xml`) et propose le lien vers l'autre langue dans la barre, le menu mobile et
le pied de page. L'anglais est la version par défaut pour les autres langues (`x-default`).

`site.js` choisit ses textes (menu, partage, téléchargement, tailles, dates) d'après
`<html lang>`. Dans `config.js`, les libellés des fichiers acceptent `{ fr: "…", en: "…" }`.

`translatedFrom` (dans `config.js`) donne la première version publiée dont l'interface est
traduite. Tant que la dernière version publiée est plus ancienne, la page anglaise affiche
une note « Languages » dans la section Téléchargement ; elle disparaît seule ensuite.
Mettre à jour ce numéro s'il change.

Pour modifier un texte : faire la même modification dans `index.html` et `en/index.html`.
Les identifiants de section sont propres à chaque langue (`#telechargement`, `#download`) ;
`data-nav-parent` rattache une section sans entrée de menu à l'entrée voisine.

## Le film

La section « Le film » (juste après l'accueil) lit le film de présentation de `promo/` dans
la langue de la page : `index.html` pointe vers les fichiers `-fr`, `en/index.html` vers
les fichiers `-en`. Rien n'est téléchargé avant le clic (`preload="none"`) ; les téléphones
reçoivent la version 720p (`<source media>`), les ordinateurs la 1080p à 60 i/s. Le bouton
« Voir le film » de l'accueil fait défiler jusqu'au film et le lance ; le film se met en pause
quand il quitte l'écran. Sans JavaScript, la vidéo garde ses commandes natives. Les fichiers
sont servis par le site lui-même (pas de plateforme vidéo tierce).

Refaire les fichiers après un nouveau rendu du film (`promo/out/`) :

```bash
for L in fr en; do
  SRC=../promo/out/halcyon-promo$( [ $L = en ] && echo -en ).mp4
  ffmpeg -i $SRC -c:v libx264 -preset slow -crf 24 -pix_fmt yuv420p -c:a aac -b:a 128k \
    -movflags +faststart assets/video/halcyon-film-$L-1080.mp4
  ffmpeg -i $SRC -c:v libx264 -preset slow -crf 24 -vf "scale=1280:720:flags=lanczos,fps=30" \
    -pix_fmt yuv420p -c:a aac -b:a 112k -movflags +faststart assets/video/halcyon-film-$L-720.mp4
  ffmpeg -ss 12.6 -i $SRC -frames:v 1 -c:v libwebp -quality 82 assets/video/halcyon-film-$L-poster.webp
done
```

## Aperçu local

```bash
cd site && python3 -m http.server 8000   # puis http://localhost:8000
```

## Téléchargements

La section Téléchargement interroge l'API publique de GitHub depuis le navigateur
(`/repos/<dépôt>/releases`) et affiche la dernière version qui contient des fichiers pour
Windows ou Linux : numéro, date, préversion, liens directs, tailles, `SHA256SUMS`. Aucune
URL de fichier n'est écrite en dur. Sans réponse de GitHub, la page renvoie vers la page
des versions.

`assets/js/config.js` choisit le dépôt, l'inclusion des préversions (à laisser à `true`
tant qu'aucune version stable n'existe) et les fichiers proposés pour chaque système
(expressions régulières sur les noms des fichiers publiés par la CI).

## Captures d'écran

`assets/img/screens/` contient de vraies captures de Halcyon 1.0.0-alpha.4 (binaire Linux
de la version publiée), prises avec une bibliothèque Steam d'exemple (`promo/tools/real-games.py` : quinze jeux
populaires et leurs visuels de bibliothèque officiels). Ces visuels appartiennent à leurs
éditeurs, ce que la page et les mentions légales indiquent. Chaque écran existe en 1440 et 2880 px de large (`-1440`, `-2880`) et en recadrage
pour téléphone (`-m`).

Les captures anglaises (`screens/en/`) viennent de la branche de traduction (interface en
anglais, même bibliothèque d'exemple, mêmes cadrages et mêmes recadrages téléphone), avant sa
publication : la page anglaise le précise sous la galerie.

## Mise en ligne

Prévu pour GitHub Pages à l'adresse `https://sebdss.github.io/halcyon/` (adresse
canonique, Open Graph, sitemap). Pour un autre domaine, remplacer cette adresse dans
les pages HTML (racine et `en/`), `robots.txt` et `sitemap.xml`, et mettre à jour
l'hébergeur dans `mentions-legales.html` et `en/legal-notice.html`.

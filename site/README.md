# Site officiel de Halcyon

Site vitrine statique, sans dépendance ni étape de compilation : HTML, CSS et JavaScript
écrits à la main. Il reprend le système de design « Berge » du launcher
(`docs/DESIGN-SYSTEM.md`) : mêmes couleurs, même police Archivo, mêmes icônes, la perche
pour l'élément actif, l'orange réservé au geste principal.

```
site/
├── index.html               page d'accueil
├── mentions-legales.html
├── confidentialite.html
├── site.webmanifest, robots.txt, sitemap.xml
└── assets/
    ├── css/site.css         jetons et composants
    ├── js/config.js         dépôt, canal, fichiers proposés  ← à modifier
    ├── js/site.js           navigation, onglets, galerie, téléchargement
    ├── fonts/               Archivo (SIL OFL), sous-ensemble latin en woff2
    └── img/                 logo, favicons, image Open Graph, captures (screens/)
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

## Mise en ligne

Prévu pour GitHub Pages à l'adresse `https://sebdss.github.io/halcyon/` (adresse
canonique, Open Graph, sitemap). Pour un autre domaine, remplacer cette adresse dans
`index.html`, les deux pages légales, `robots.txt` et `sitemap.xml`, et mettre à jour
l'hébergeur dans `mentions-legales.html`.

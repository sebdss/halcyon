# Halcyon 1.0.0-alpha.7 — Étagère coverflow, musée, dos de boîtiers

Rien n'est retiré : toutes les fonctions de la 1.0.0-alpha.6 restent. Aucune migration de
base ; trois réglages s'ajoutent (`museum_lighting`, `museum_seen`, `launchbox_art`), avec
des valeurs par défaut.

Guide : [`GUIDE.md`](GUIDE.md) (section 6) · design : [`DESIGN-SYSTEM.md`](DESIGN-SYSTEM.md) ·
sécurité : [`SECURITE.md`](SECURITE.md) (§ 4 ter) · mesures :
[`PERFORMANCES.md`](PERFORMANCES.md) · tests : [`TESTS-1.0.0-alpha.7.md`](TESTS-1.0.0-alpha.7.md).

## Ce qui change pour vous

### Une étagère façon coverflow
- Le jeu choisi trône au centre, presque de face, dans un halo de la couleur de l'étagère ;
  les autres boîtiers reculent en pile, tournés vers lui. Sur ceux de gauche, on lit les
  tranches.
- Reflets de boîtier en plastique, voisins dans la pénombre.
- **Y** (ou **F** au clavier) **retourne le boîtier** : son vrai dos si Halcyon l'a récupéré,
  sinon un dos composé avec l'image du jeu, son résumé, ses genres, son développeur, sa date et
  votre temps de jeu.

### Le musée
- Troisième vue de la section *Jouer* (R3 ou `V` : rangée → étagère → musée).
- Toute la collection en mur d'étagères, une rangée par plateforme, les boîtiers tranche vers
  l'avant ; une plaque par rangée avec le nombre de jeux et le temps de jeu.
- Le boîtier choisi sort de la rangée et se tourne vers vous. Les jeux ajoutés depuis votre
  dernière visite brillent. `L` change l'éclairage : galerie, jour, néon.

### Vrais dos et vraies tranches (facultatif)
- *Paramètres › Comportement › Dos et tranches de boîtiers (LaunchBox)*, désactivé par défaut.
  Un téléchargement unique d'environ 110 Mo, ensuite seulement un petit index sur le disque ;
  les images arrivent au fil de la navigation, ou toutes d'un coup avec « Récupérer pour toute
  la bibliothèque ».
- Surtout utile pour les jeux console (ROM) et les éditions en boîte ; les autres gardent leur
  dos et leur tranche composés.

### Toujours respecté
- Mouvement réduit ou aucun (le halo ne respire plus), mode Performance (jaquettes à plat,
  dos d'un coup), texte jusqu'à 130 %, de 800 × 560 à la 4K, jeux sans jaquette, rendu
  logiciel (titres des tranches en lettres empilées).

## Bon à savoir
- Dans l'étagère et le musée, **Y retourne le boîtier** au lieu d'ouvrir la recherche ; la
  recherche reste dans la barre des sections (haut) et sur `Tab`.
- Idées reprises de [Boxshelf](https://github.com/sassiestrhihno/boxshelf-source) (étagère
  avec halo et reflets, boîtier retourné, musée) ; aucun code n'en vient : Boxshelf est sous
  GPL-3.0 et utilise un moteur web et WebGL, Halcyon reste en Slint natif, sous licence MIT.
  Pas de vue « Inspecter » en 3D ni de musée en 3D réelle : il faudrait un moteur 3D, trop
  lourd pour Halcyon.

## Non vérifié faute de matériel ou de réseau
- Le téléchargement réel de la base LaunchBox et de ses images (réseau limité dans
  l'environnement de développement) : lecture de l'archive, index et contrôles sont testés sur
  des données de test, l'affichage sur des scans de test.
- Fluidité avec une vraie carte graphique, sous Windows, et à la manette réelle (les mêmes
  chemins ont été vérifiés au clavier).

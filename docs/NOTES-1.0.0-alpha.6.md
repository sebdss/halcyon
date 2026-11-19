# Halcyon 1.0.0-alpha.6 — Collections, étagère, désinstallation

Rien n'est retiré : toutes les fonctions de la 1.0.0-alpha.5 restent, et leurs données sont
reprises telles quelles. La base passe en **v7** par ajout de colonnes (couleur, logo,
bannière et positions des collections) ; les collections existantes gardent leurs jeux, dans
l'ordre alphabétique tant que vous n'en choisissez pas un autre. Une base v7 reste lisible par
la 1.0.0-alpha.5.

Guide : [`GUIDE.md`](GUIDE.md) (sections 3 bis, 3 ter, 6) · design :
[`DESIGN-SYSTEM.md`](DESIGN-SYSTEM.md) · sécurité : [`SECURITE.md`](SECURITE.md) (§ 4 bis) ·
mesures : [`PERFORMANCES.md`](PERFORMANCES.md#100-alpha6--étagère) · tests :
[`TESTS-1.0.0-alpha.6.md`](TESTS-1.0.0-alpha.6.md).

## Ce qui change pour vous

### Des collections en quelques gestes
- **Cochez plusieurs jeux** dans la bibliothèque (`Ctrl`-clic, `Maj`-clic pour une plage,
  `Ctrl+Espace`, ou **R3** à la manette), puis **Ajouter à une collection…**.
- **Nouvelle collection…** : tapez « sonic », les jeux correspondants sont listés et cochés ;
  décochez ceux qui n'en font pas partie, validez.
- **Séries détectées** : Halcyon repère les titres au début commun (Resident Evil 2, 3, 4,
  Village…) et propose la collection. Rien n'est créé sans votre clic ; « Ignorer » est
  retenu.
- Une collection mélange **toutes les sources** : Steam, GOG, Epic, jeux émulés, jeux
  autonomes.
- **Page Collections** : nom, couleur, logo, bannière, ordre des jeux (boutons ↑ ↓, ou à la
  manette : prendre, déplacer, poser) et place de chaque étagère.

### L'étagère, seconde vue du mode console
- **R3** (ou `V`) bascule la section *Jouer* entre la rangée de jaquettes et l'étagère ; la
  vue est retenue.
- Une étagère par collection, plus Récents, Favoris et Tous, chacune avec sa plaque de
  couleur. Gauche / droite pour les jeux, haut / bas pour les étagères, A pour jouer, X pour
  les favoris, Select pour le menu du jeu.
- Chaque jeu est un boîtier, rangé vu de côté : le titre sur la tranche, la couleur de sa
  plateforme ou de sa source. Le jeu choisi avance, se tourne pour montrer sa jaquette, ses
  voisins s'écartent, et sa bannière apparaît au fond.
- Vos réglages sont respectés : mouvement réduit ou aucun, mode Performance (jaquettes à
  plat), texte jusqu'à 130 %, petites fenêtres comme écrans 4K, jeux sans jaquette.

### Désinstaller sans quitter Halcyon
- **Désinstaller…** dans le menu d'un jeu (bibliothèque, mode console, étagère) et dans
  *Outils › Stockage* pour les jeux non lancés depuis six mois.
- Halcyon passe par la plateforme : Steam, Legendary, ou la page du launcher (Heroic,
  launcher Epic, GOG Galaxy, Battle.net ; *Applications* de Windows pour les autres). Un jeu
  autonome va à la **corbeille**. Une ROM ou un jeu ajouté à la main n'est jamais supprimé :
  il est masqué ou retiré de la bibliothèque.
- Toujours une confirmation (nom, taille, emplacement) avec **Sauvegarder avant** ; le bouton
  actif au départ est *Annuler*, même à la manette. Le jeu reste ensuite dans la bibliothèque
  comme « Non installé », avec ses favoris, son temps de jeu et ses collections.

## Ce qui a été vérifié, et ce qui ne l'a pas été

Vérifié sous Linux (Xvfb, captures, rendu OpenGL émulé et rendu logiciel) : sélection
multiple à la souris et au clavier, création guidée, collection mixte (Steam, SNES, Epic),
suggestions et refus, page Collections (couleur, ordre, place), étagère vide, collection de
3 jeux, bibliothèque de 500 jeux, jeux sans jaquette, mode Performance, texte à 130 %,
800 × 560 et 3840 × 2160, désinstallation d'un jeu autonome vers la corbeille, délégation à
Steam, annulation, page Stockage. Migration v6 → v7 testée sur une base de test.

**Non vérifiable ici, à essayer sur votre PC Windows** : Corbeille Windows (jeu autonome),
ouverture de Steam, du launcher Epic, de GOG Galaxy et de Battle.net sur leur page,
*Applications* de Windows, `legendary uninstall` avec de vrais jeux, et la manette réelle
(bouton R3 et navigation dans le dialogue ; testée ici par les mêmes chemins au clavier).

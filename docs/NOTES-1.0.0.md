# Halcyon 1.0.0-alpha.1 — « Performance First »

Première étape de la 1.0. Rien n'est retiré : bibliothèque, Big Picture, émulateurs,
succès, sauvegardes, mods, son, installateur, Minecraft et toutes les fonctions de la 0.5.0
sont inchangés, et leurs données sont reprises telles quelles (base migrée en v6 par ajout
d'une table ; une base v6 reste lisible par la 0.5.0).

Audit de départ : [`AUDIT-1.0.md`](AUDIT-1.0.md) · sécurité : [`SECURITE.md`](SECURITE.md) ·
mesures : [`PERFORMANCES.md`](PERFORMANCES.md#100-alpha1) · tests :
[`TESTS-1.0.0.md`](TESTS-1.0.0.md).

## Ce qui change pour vous

### Nouvelle apparence « Alcyon »
- Halcyon change de visage : bleu d'eau profonde, accent cobalt, orange pour ce qui est en
  cours, et une « ligne d'horizon » qui montre où vous êtes. Plus de capitales partout,
  titres plus lisibles, contrastes vérifiés. L'ancienne apparence reste disponible
  (Apparence › Thème › « Nocturne (0.5) »).

### Nouvelle navigation et nouveau mode console
- **Un menu de six espaces** au lieu de 19 entrées : Accueil, Bibliothèque, Minecraft,
  Activité, Outils, Système. Activité, Outils et Système ont leurs onglets en haut de page,
  avec les compteurs utiles (téléchargements, mises à jour, jeux émulés, manettes).
  Rien n'a disparu : chaque page de la 0.5 est à un clic de son espace.
- **Mode console** pensé pour la télévision et la manette : le jeu sélectionné occupe tout
  l'écran, une rangée de jaquettes en dessous, une fiche plein écran, un menu d'options par jeu
  et un centre de contrôle (touche Menu / Select). Voir le Guide, section 6.
- **Accueil, fiche de jeu, bibliothèque et paramètres** repris : grandes illustrations sans
  raccord, titres en Sora, paramètres classés par catégorie dans une colonne à gauche.

### Mises à jour automatiques (nouveau)
- Halcyon se met à jour tout seul : la nouvelle version est téléchargée et vérifiée en
  arrière-plan, installée au démarrage suivant, puis Halcyon redémarre. Si elle ne démarre pas
  correctement, l'ancienne version revient automatiquement.
- Canal **Stable** ou **Beta** dans *Paramètres › Mises à jour*, avec la progression réelle,
  l'annulation et les notes de version.
- Linux : nouveau format **AppImage**, installable pour votre compte depuis les paramètres.
- Détails : [`HUS.md`](HUS.md).

### Performance Center (nouveau)
- Nouvel onglet **Performances** dans l'espace Système (et dans le lancement rapide, Ctrl+K).
- Processeur, mémoire, carte(s) graphique(s), stockage en direct, mesurés uniquement
  pendant que la page est ouverte.
- Applications les plus gourmandes, regroupées par application, et coût de Halcyon lui-même.
- Recommandations tirées des mesures ; une seule action proposée, réversible (mode
  Performance de Halcyon). Halcyon ne ferme aucun programme et ne touche à aucun réglage du
  système.
- Historique des performances de chaque partie (processeur, mémoire, carte graphique),
  désactivable.
- FPS, « 1 % low », « 0,1 % low » et courbe des temps d'image à partir des journaux
  **MangoHud** (Linux) et **PresentMon** (Windows) : aucune injection dans les jeux.

### Sécurité
- **Clés d'API retirées de `settings.json`** : rangées dans un coffre chiffré par Windows
  (DPAPI) ou réservé à votre compte sous Linux. Déplacement automatique au premier lancement.
- `settings.json` n'est plus lisible par les autres comptes de l'ordinateur sous Linux.
- **Mises à jour signées** (Ed25519) : une version officielle sans signature valide est
  refusée. Le niveau de vérification est affiché dans Paramètres › Mise à jour.
- **Journal de sécurité** `logs/security.log` : clés définies ou retirées (jamais leur
  valeur), extensions et autorisations, mises à jour vérifiées ou refusées, compte
  Microsoft, profils du launcher Minecraft officiel.

### Bibliothèque : GOG
- Jeux GOG installés par Heroic (Linux, Windows) ou GOG Galaxy (Windows), avec titres et
  visuels du cache Heroic. Filtre « GOG » et règle de collection intelligente.

### Téléchargements
- Nouvelle page : vitesse, file d'attente, pause et reprise de tout ce que Halcyon
  télécharge, limite de débit, nombre de téléchargements simultanés. Un fichier interrompu
  par une coupure reprend où il s'était arrêté.

### Statistiques, actualités, extensions, profils
- Export des statistiques en CSV (tableurs) ou JSON.
- Actualités officielles des jeux Steam dans leur fiche.
- Extensions : empreinte du contenu, signature de l'auteur, mise à jour refusée si elle
  n'est pas signée par le même auteur.
- Profils d'interface : « Bureau », « Salon »… enregistrés et appliqués d'un clic.

### Corrections
- L'installateur proposait « Réinstaller » au lieu de « Mettre à jour » pour une pré-version.
- Une pré-version (`1.0.0-alpha.1`) est désormais considérée comme plus ancienne que la
  version finale correspondante : sans cela, une installation de test n'aurait jamais
  proposé la 1.0.0.
- Minecraft apparaît dans le lancement rapide (Ctrl+K).

## Ce qui n'est pas fait

- Contenu de certaines pages encore proche de la 0.5 (Sauvegardes, Jeu à distance, Manettes,
  Extensions, Journal, onglets de la fiche) : elles profitent de la nouvelle navigation, des
  titres et des composants communs, sans mise en page propre.
- Le binaire complet ne se compile pas dans l'environnement de développement utilisé
  (mémoire) : les vérifications à l'écran ont été faites sur un binaire de test sans la page
  Minecraft, et la page Minecraft a été vérifiée par `cargo check` uniquement.
- Validation sur un vrai PC Windows et sous Linux avec réseau (voir
  [`TESTS-1.0.0.md`](TESTS-1.0.0.md)).
- Succès GOG : pas d'API publique sans connexion au compte (succès personnalisés seulement).

## À faire une fois (auteur)

**Mises à jour automatiques** : sans clé de signature, Halcyon signale les nouvelles versions
mais ne les installe pas. Créez la paire de clés, ajoutez la clé publique à
`crates/halcyon-update/keys/release.pub`, puis créez les environnements GitHub
`release-stable` (avec relecteurs) et `release-beta` contenant le secret
`HALCYON_SIGNING_KEY` : [`HUS.md`](HUS.md#5-mise-en-place-des-clés-une-seule-fois).


Créer la paire de clés de signature et les deux réglages GitHub décrits dans
[`SECURITE.md`](SECURITE.md#mise-en-place-une-seule-fois-par-lauteur). Sans eux, la
version est publiée avec la vérification d'intégrité seule (comme en 0.5.0) et la CI
l'indique par un avertissement.

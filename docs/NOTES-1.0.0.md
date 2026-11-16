# Halcyon 1.0.0-alpha.1 — « Performance First »

Première étape de la 1.0. Rien n'est retiré : bibliothèque, Big Picture, émulateurs,
succès, sauvegardes, mods, son, installateur, Minecraft et toutes les fonctions de la 0.5.0
sont inchangés, et leurs données sont reprises telles quelles (base migrée en v6 par ajout
d'une table ; une base v6 reste lisible par la 0.5.0).

Audit de départ : [`AUDIT-1.0.md`](AUDIT-1.0.md) · sécurité : [`SECURITE.md`](SECURITE.md) ·
mesures : [`PERFORMANCES.md`](PERFORMANCES.md#100-alpha1) · tests :
[`TESTS-1.0.0.md`](TESTS-1.0.0.md).

## Ce qui change pour vous

### Performance Center (nouveau)
- Nouvelle entrée **Performances** dans le menu (et dans le lancement rapide, Ctrl+K).
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

- Refonte visuelle des écrans existants (les nouveaux écrans suivent le design system).
- Validation sur un vrai PC Windows et sous Linux avec réseau (voir
  [`TESTS-1.0.0.md`](TESTS-1.0.0.md)).
- Succès GOG : pas d'API publique sans connexion au compte (succès personnalisés seulement).

## À faire une fois (auteur)

Créer la paire de clés de signature et les deux réglages GitHub décrits dans
[`SECURITE.md`](SECURITE.md#mise-en-place-une-seule-fois-par-lauteur). Sans eux, la
version est publiée avec la vérification d'intégrité seule (comme en 0.5.0) et la CI
l'indique par un avertissement.

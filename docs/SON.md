# Identité sonore de Halcyon (0.4.1)

Halcyon a une signature sonore propre : des sons courts, feutrés et cristallins, construits
autour d'un même motif (ré – la – mi, quinte puis neuvième). Ils accompagnent l'interface
sans la couvrir et ne jouent jamais pendant une partie, sauf si vous le demandez.

## Origine et licence

- Les 28 sons sont **synthétisés** par `tools/sound/make_sounds.py` (NumPy). Aucun
  échantillon, aucune banque de sons, aucun son d'une autre marque n'est utilisé.
- Le script est déterministe (graine fixe) : relancé, il produit les mêmes fichiers octet
  pour octet.
- Licence : MIT, comme le reste de Halcyon.

```sh
pip install numpy --break-system-packages   # une seule dépendance, pour la génération
python3 tools/sound/make_sounds.py          # régénère crates/halcyon-audio/sounds/
```

Les fichiers WAV (PCM 16 bits, mono, 44,1 kHz, environ 1 Mo en tout) sont intégrés au
binaire : rien n'est lu depuis le disque, rien n'est à installer.

### Construction

| Brique | Rôle |
|---|---|
| Cloche (partiels inharmoniques) | Le timbre « verre » de la signature |
| Glissando | Ouverture et fermeture des panneaux, menus |
| Bruit filtré à balayage | Souffle des transitions (centre de commandes, mode) |
| Clic | Navigation (sélection, survol) |
| Réverbération courte | Espace commun à tous les sons, coupée à −46 dB |
| Finition | Fondus d'entrée et de sortie, sonie par catégorie, crête ≤ −3 dBFS |

Niveaux visés (fenêtre de 30 ms la plus forte) : navigation −24 dB, interaction −19 dB,
notification −16 dB, important −14 dB. Le script vérifie que chaque son commence et finit à
zéro, ne sature pas et n'a pas de composante continue.

## Catalogue

| Identifiant | Catégorie | Moment |
|---|---|---|
| `ui.select` | Navigation | Sélection d'un élément |
| `ui.hover` | Navigation | Survol (désactivé par défaut) |
| `ui.section` | Navigation | Changement de section |
| `ui.panel-open` / `ui.panel-close` | Navigation | Recherche rapide ouverte / fermée |
| `ui.menu-open` / `ui.menu-close` | Navigation | Menu contextuel |
| `ui.back` | Navigation | Retour (Alt+←, bouton B) |
| `game.select` | Navigation | Jeu sélectionné au clavier ou à la manette |
| `ui.confirm` | Interaction | Validation d'une confirmation, d'une action Big Picture |
| `game.open` | Interaction | Ouverture d'une fiche |
| `game.favorite` | Interaction | Ajout aux favoris |
| `game.collection` | Interaction | Ajout à une collection, création |
| `game.launched` | Interaction | Le jeu lancé par sa plateforme a démarré |
| `ui.command-center` | Interaction | Centre de commandes |
| `ui.mode` | Interaction | Mode d'affichage, entrée en Big Picture |
| `ui.toggle-on` / `ui.toggle-off` | Interaction | Interrupteurs, mode Performance |
| `ui.window` | Interaction | Fenêtre importante (confirmation, ajout de jeu) |
| `ui.setting` | Interaction | Réglage modifié (choix segmentés) |
| `notify.success` | Notification | Export de configuration réussi |
| `notify.warning` | Notification | Perte de connexion |
| `notify.error` | Notification | Message d'erreur |
| `notify.download` | Notification | Mise à jour de Halcyon téléchargée et vérifiée |
| `notify.install` | Notification | Nouveaux jeux trouvés par l'analyse |
| `notify.update` | Notification | Téléchargement Steam terminé |
| `game.launch` | Important | Lancement d'un jeu |
| `notify.major` | Important | Succès débloqué |

**Media Center** : Halcyon n'a pas de Media Center. Aucun son n'a été créé pour une
fonction qui n'existe pas ; la catégorie pourra être ajoutée avec la fonction.

## Moteur (`crates/halcyon-audio`)

- Un seul point d'entrée : `Controller::sound(Cue)` côté Rust, `Sound.play("ui.select")`
  côté interface. Les évènements de l'interface et du contrôleur passent tous par lui.
- Fil dédié `halcyon-audio` : l'interface n'attend jamais le son. Le périphérique n'est
  ouvert qu'au premier son, et refermé après 20 s de silence (aucun réveil au repos).
- Sortie par `cpal` (WASAPI sous Windows, ALSA sous Linux, PipeWire et PulseAudio passant par
  leur couche ALSA). Sans périphérique, Halcyon reste muet et réessaie au plus une fois par
  minute ; aucune erreur n'est affichée à l'utilisateur, une ligne est écrite au journal.
- Mélangeur : 6 voix au plus, 1 à 2 par catégorie ; une voix de navigation redémarre au
  lieu de s'empiler ; au-delà, la voix la moins prioritaire puis la plus ancienne est
  remplacée.
- Anti-répétition : chaque son a un délai minimal (45 ms pour la sélection, 1,5 s pour un
  lancement), ce qui évite la « mitraillette » en maintenant une flèche.
- Pendant une partie : silencieux (défaut), réduit (30 %) ou normal.

## Réglages (Paramètres › Son)

Activation, mode silencieux, volume général, volumes par catégorie (navigation,
interaction, notifications, important), bouton « Écouter » par curseur, son au survol,
comportement pendant une partie, réinitialisation. Les réglages sont enregistrés avec les
autres paramètres (`settings.json`, section `sound`) ; un fichier de la 0.4.0 sans cette
section prend les valeurs par défaut.

## Limites

- La sortie réelle a été vérifiée sous Linux (ALSA, rendu dans un fichier) ; la machine de
  développement n'a pas de carte son, l'écoute a donc été faite sur les fichiers WAV.
  Windows (WASAPI) n'a pas pu être testé ici.
- `game.launched` est muet par défaut, puisqu'il survient quand la partie commence (réglage
  « Silencieux »).

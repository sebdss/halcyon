# Créer une extension Halcyon

Une extension ajoute à Halcyon des **données** : émulateurs, plateformes, emplacements de
sauvegarde, thèmes, dossiers de jeux, sources de métadonnées et liens. Elle tient dans un
dossier contenant un fichier `plugin.json`. Ce dossier peut être distribué tel quel ou dans une
archive `.zip`. Une archive renommée en `.halcyonplugin` s'ouvre aussi d'un double-clic.

Un exemple complet se trouve dans [`examples/plugins/liens-et-theme`](../examples/plugins/liens-et-theme/plugin.json).

## Principes de sécurité

| Règle | Conséquence |
|---|---|
| **Aucun code** | Halcyon n'exécute jamais le contenu d'une extension. Une archive contenant un fichier exécutable ou un script (`.exe`, `.dll`, `.so`, `.dylib`, `.bat`, `.cmd`, `.ps1`, `.sh`, `.msi`, `.scr`, `.com`, `.vbs`, `.js`, `.py`) est **refusée** à l'installation. |
| **Installée désactivée** | Après installation, l'extension reste inactive. L'activer affiche un résumé de ses autorisations et de ses apports, à confirmer. |
| **Réseau limité** | Une source de métadonnées ne peut interroger que les domaines déclarés par `network:<domaine>`, en HTTPS uniquement. Une URL hors de ces domaines est ignorée. Les requêtes respectent le mode hors ligne. |
| **Dossiers explicites** | Les dossiers de jeux ne sont analysés qu'avec la permission `folders`. |
| **Liens sûrs** | Seuls les liens `https://` sont affichés. Ils s'ouvrent dans le navigateur. |
| **Rien d'écrasé** | Une extension ne peut pas remplacer un émulateur, une plateforme ou un thème intégré. |
| **Mise à jour = nouvelle validation** | Si une nouvelle version demande d'autres autorisations, elle est désactivée jusqu'à une nouvelle confirmation. |

## Format de `plugin.json`

```json
{
  "id": "mon-extension",
  "name": "Mon extension",
  "version": "1.0.0",
  "author": "Votre nom",
  "description": "Ce que l'extension apporte, en une phrase.",
  "homepage": "https://exemple.org/mon-extension",
  "min_halcyon": "0.3.0",
  "permissions": ["network:api.exemple.org", "folders"],
  "update_url": "https://exemple.org/mon-extension/latest.json",
  "provides": {
    "emulators": [],
    "platforms": [],
    "save_locations": [],
    "themes": [],
    "game_folders": [],
    "metadata": [],
    "links": []
  }
}
```

| Champ | Obligatoire | Description |
|---|---|---|
| `id` | oui | Lettres, chiffres, `-` et `_`. Sert de nom de dossier. |
| `name`, `version` | oui | Affichés dans la page Extensions. |
| `author`, `description`, `homepage` | non | `homepage` doit être en `https://`. |
| `min_halcyon` | non | Version minimale de Halcyon. Une version plus ancienne refuse l'extension. |
| `permissions` | non | `network:<domaine>` (sous-domaines inclus) et `folders`. |
| `update_url` | non | Adresse HTTPS d'un JSON `{"version": "1.1.0"}`. Halcyon signale la mise à jour et ne télécharge rien lui-même. |

## Apports (`provides`)

### Émulateurs et plateformes

Ils utilisent le même format que les définitions intégrées
(`crates/halcyon-integrations/data/emulators.json` et `platforms.json`). Exemple :

```json
"platforms": [
  { "id": "vectrex", "name": "Vectrex", "short": "VEC", "extensions": ["vec", "bin"] }
],
"emulators": [
  {
    "id": "vecx",
    "name": "Vecx",
    "executables": { "windows": ["vecx.exe"], "linux": ["vecx"] },
    "search_paths": { "windows": ["%PROGRAMFILES%\\Vecx"], "linux": ["/usr/bin", "~/.local/bin"] },
    "platforms": { "vectrex": { "args": ["{rom}"] } },
    "fullscreen_args": ["--fullscreen"]
  }
]
```

Les chemins de recherche acceptent `%VARIABLE%` (Windows) et `~`. Gabarits disponibles dans les arguments : `{rom}`, `{rom_dir}`, `{rom_name}`, `{core_dir}`,
`{lib_ext}` et `{emu_dir}`. Un identifiant intégré (`retroarch`, `snes`…) est refusé.

### Emplacements de sauvegarde

```json
"save_locations": [
  { "game": "Hollow Knight", "paths": ["{locallow}/Team Cherry/Hollow Knight"] }
]
```

Le nom du jeu est comparé sans tenir compte de la casse, des accents ni des espaces. Jetons
disponibles :

| Jeton | Contenu |
|---|---|
| `{home}` | dossier personnel |
| `{documents}` | Documents |
| `{savedgames}` | Parties enregistrées (Windows) |
| `{appdata}` | AppData\Roaming |
| `{localappdata}` | AppData\Local |
| `{locallow}` | AppData\LocalLow |
| `{config}` | `~/.config` |
| `{data}` | `~/.local/share` |
| `{steam}` | dossier Steam |
| `{install}` | dossier d'installation du jeu |

Les emplacements fournis apparaissent comme **suggestions** dans l'onglet Sauvegardes du jeu.
Le joueur choisit de les associer ou non.

### Thèmes

```json
"themes": [
  {
    "id": "nuit-polaire",
    "name": "Nuit polaire",
    "colors": { "bg": "#0e1117", "surface": "#181d26", "accent": "#88c0d0", "on_accent": "#0b1a20" }
  }
]
```

Couleurs possibles : `bg`, `sidebar`, `surface`, `surface_hover`, `surface_active`, `border`,
`border_strong`, `text`, `text_muted`, `text_dim`, `accent`, `on_accent`, `warm`, `danger`,
`success` et `bp_bg` (fond du mode Big Picture). Elles s'écrivent `#rrggbb` ou `#rrggbbaa`.
Une couleur absente reprend celle du thème Halcyon. Le même format sert aux fichiers
`.halcyontheme` importés depuis la page Apparence.

### Dossiers de jeux

```json
"permissions": ["folders"],
"provides": { "game_folders": ["{home}/Games/itch", "~/Jeux/GOG"] }
```

Ces dossiers sont ajoutés à l'analyse des jeux standalone tant que l'extension est active.

### Sources de métadonnées

```json
"permissions": ["network:api.exemple.org"],
"provides": {
  "metadata": [
    {
      "name": "Exemple DB",
      "url": "https://api.exemple.org/jeux?nom={name}",
      "fields": {
        "description": "/results/0/summary",
        "developer": "/results/0/studio",
        "genres": "/results/0/genres",
        "release_date": "/results/0/date"
      }
    }
  ]
}
```

- Le gabarit d'URL accepte `{name}` (nom du jeu encodé) et `{steam_appid}`.
- Chaque champ est un **pointeur JSON** (RFC 6901) vers la valeur dans la réponse. Champs
  reconnus : `description`, `developer`, `publisher`, `release_date`, `genres`, `tags` et
  `screenshots` (URL HTTPS).
- La source ne sert qu'aux jeux **sans description**. Elle ne remplace jamais une valeur que
  vous avez modifiée à la main. Une réponse vide n'est pas redemandée avant plusieurs jours.
- N'utilisez que des services dont les conditions autorisent ce type d'accès. Une clé d'API
  personnelle ne doit jamais figurer dans une extension distribuée.

### Liens

```json
"links": [
  { "label": "PCGamingWiki", "url": "https://www.pcgamingwiki.com/w/index.php?search={name}" },
  { "label": "SteamDB", "url": "https://steamdb.info/app/{steam_appid}/" }
]
```

Les liens s'affichent dans la fiche des jeux. Un lien contenant `{steam_appid}` n'apparaît
que pour les jeux Steam.

## Tester et distribuer

1. Créez le dossier avec son `plugin.json`.
2. Dans **Extensions › Installer**, indiquez le dossier puis confirmez. Les erreurs de
   validation s'affichent sous le champ.
3. Activez l'extension, puis vérifiez ses apports : thème dans Apparence, liens dans une fiche,
   émulateurs dans Émulateurs.
4. Pour distribuer, zippez le dossier, ou son contenu, et renommez éventuellement l'archive en
   `.halcyonplugin`.

Les extensions installées se trouvent dans `<données>/plugins/<id>`. Elles sont incluses dans
l'export de configuration.

## Limites volontaires

- Pas de code : pas de nouveau type de source de jeux, de fournisseur de succès ni d'action
  arbitraire. Ces besoins passent par une contribution au code de Halcyon, relue et testée.
- Pas de succès fournis par une extension : Halcyon n'affiche que des succès issus d'une
  plateforme officielle (Steam, RetroAchievements) ou créés par le joueur.

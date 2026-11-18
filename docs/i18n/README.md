# Traductions de Halcyon

Le launcher et l'installateur sont traduits en six langues : français (référence), anglais
(repli), allemand, espagnol, italien et portugais (Portugal). Les traductions sont des
fichiers gettext `.po` dans `locales/`, intégrés aux exécutables à la compilation : aucune
connexion, aucun service de traduction, aucun texte de l'utilisateur envoyé où que ce soit.

- Règles de ton, de typographie et lexique commun : [`GLOSSAIRE.md`](GLOSSAIRE.md).
- Audit initial et bilan final : [`AUDIT.md`](AUDIT.md).

## Architecture

```
locales/
  templates/<domaine>.pot     textes à traduire (générés, ne pas modifier)
  <langue>/<domaine>.po       une langue : de-DE, en-US, es-ES, fr-FR, it-IT, pt-PT
crates/halcyon-i18n/          gestionnaire de langue (Rust, sans dépendance lourde)
tools/i18n/i18n.py            extraction, avancement, vérification des oublis
```

**Clés.** Le texte français écrit dans le code est la clé (`msgid`). Un contexte
(`msgctxt`) distingue deux sens d'un même mot (`trc!("month", "mai")`).

**Domaines.** Les textes sont répartis par zone de l'application (`navigation`, `home`,
`library`, `games`, `activity`, `tools`, `minecraft`, `console`, `system`, `formats`,
`common`, `installer`) pour que chaque fichier reste lisible. À la compilation,
`halcyon_i18n::build::assemble` fusionne les domaines d'un exécutable en un catalogue par
langue (le launcher prend tout sauf `installer`, l'installateur ne prend que `installer`) et
complète les traductions manquantes par l'anglais.

**Deux moteurs, un format.**
- Slint : `@tr("…")` dans les `.slint`, traduit par Slint à partir des mêmes `.po`
  (`with_bundled_translations`). `slint::select_bundled_translation()` change la langue à chaud.
- Rust : macros de `halcyon-i18n`, pour les textes construits par les contrôleurs
  (notifications, erreurs, modèles, formats).

| Besoin | Rust | Slint |
|---|---|---|
| Texte simple | `tr!("Paramètres")` | `@tr("Paramètres")` |
| Variables nommées | `tr!("{name} est prêt", name = g.name)` | `@tr("{} est prêt", name)` (positionnelles) |
| Pluriel | `trn!(n, "{n} jeu", "{n} jeux")` (n formaté selon la langue) | `@tr("{n} jeu" \| "{n} jeux" % n)` |
| Contexte | `trc!("month", "mai")` | `@tr("month" => "mai")` |
| Texte d'un tableau statique | `tr_noop!("Accueil")` puis `t(label)` à l'affichage | — |
| Texte enregistré, relu plus tard (journal, état de mise à jour) | `tr_later!(…)` puis `render(&texte)` | — |
| Nombres | `number(12500)` → « 12 500 », « 12,500 », « 12.500 » ; `decimal(48.25, 1)` | — |
| Accolades littérales | `{{` `}}` | `{{` `}}` |

Jamais de phrase assemblée par morceaux (`"Il reste " + n + " fichier(s)"`) : une phrase
entière par clé, avec ses variables, et un vrai pluriel. L'ordre des mots appartient au
traducteur.

**Choix de la langue** (`halcyon_i18n::resolve`, appelé au démarrage) :
1. préférence enregistrée dans `settings.json` (`"language": "de-DE"`), si disponible ;
2. au premier lancement seulement, langue choisie dans l'installateur
   (`install-language.txt` à côté de l'exécutable), enregistrée comme préférence ;
3. langues du système dans l'ordre : code exact, puis même langue d'une autre région
   (`de-AT` → `de-DE`, `pt-BR` → `pt-PT`, `es-MX` → `es-ES`) ;
4. sinon l'anglais.

« Automatique (langue du système) » dans Système › Paramètres › Langue efface la préférence.

**Changement à chaud.** `ctl_lang.rs` : enregistre la préférence, active la langue côté Rust
et Slint, puis `relocalize()` recalcule les modèles produits par Rust (bibliothèque,
accueil, réglages, tâches, sons, palette, menu de notification) et réaffiche la page
courante. La page et la section affichées restent les mêmes, les données ne sont pas
rechargées : aucun redémarrage n'est nécessaire. Dans l'installateur, le sélecteur de l'écran d'accueil
retraduit toutes les étapes suivantes ; la langue choisie est écrite dans le dossier
d'installation à la fin.

**Ce qui n'est pas traduit, volontairement.** Noms de jeux, fiches, actualités et succès
venant des boutiques ; noms de marques ; journaux techniques (`log::`) ; noms donnés par
l'utilisateur. Les collections intelligentes proposées par défaut sont enregistrées sous
leur nom source et suivent donc la langue, tant qu'elles ne sont pas renommées.

**Écriture de droite à gauche.** Non prise en charge aujourd'hui, mais rien ne l'empêche :
aucun texte dans les images, aucune position calculée selon le sens d'écriture côté Rust,
et l'en-tête des `.po` peut porter une future métadonnée de direction.

## Ajouter une langue

Exemple : néerlandais.

```sh
# 1. Créer les fichiers (nom natif, règle de pluriel gettext, séparateur de milliers, décimal)
python3 tools/i18n/i18n.py add-language nl-NL "Nederlands" "nplurals=2; plural=(n != 1);" "." ","

# 2. Traduire les msgstr de locales/nl-NL/*.po (Poedit, Lokalize ou un éditeur de texte),
#    en suivant docs/i18n/GLOSSAIRE.md. Laisser vide ce qui n'est pas traduit : l'anglais
#    s'affichera à la place. Retirer la marque « fuzzy » une fois une entrée relue.

# 3. Vérifier puis compiler
python3 tools/i18n/i18n.py status
cargo test -p halcyon-i18n --test catalogs
cargo build --release
```

Puis ajouter la ligne de la langue dans `LANGUAGES` de `tools/i18n/i18n.py` (pour que
`extract` la conserve avec ses métadonnées), et `nl-NL` dans la liste `LANGUAGES` de
`crates/halcyon-i18n/tests/catalogs.rs` et dans le test `all_six_languages_are_bundled`
de `crates/halcyon-app/src/lang.rs`. Rien d'autre à modifier dans le code : la langue
apparaît dans les deux sélecteurs, et la détection du système la reconnaît.

Langues à plus de deux formes de pluriel (polonais, russe, arabe…) : la règle gettext
complète est acceptée (`nplurals=3; plural=(n==1 ? 0 : n%10>=2 && n%10<=4 && (n%100<10 || n%100>=20) ? 1 : 2);`),
avec un `msgstr[i]` par forme.

## Travailler avec les textes

```sh
python3 tools/i18n/i18n.py extract   # après tout ajout ou changement de texte
python3 tools/i18n/i18n.py status    # avancement par langue
python3 tools/i18n/i18n.py lint      # textes visibles écrits sans @tr / tr!
```

- `extract` relit le Slint et le Rust, régénère `templates/` et met à jour chaque langue :
  nouvelles entrées vides, entrées disparues retirées, traductions existantes conservées.
- Modifier un texte français change sa clé : la traduction doit être refaite dans chaque
  langue (le test de catalogue le signale).
- `lint` signale tout texte en français qui n'est pas passé par la traduction. Les
  exceptions voulues (marques, identifiants internes, exemples de commandes) sont dans
  `tools/i18n/lint-allow.txt`, au format `texte`, `chemin:texte` ou `chemin:*`, avec un
  commentaire qui explique pourquoi.

## Tests

| Test | Vérifie |
|---|---|
| `cargo test -p halcyon-i18n --test catalogs` | fichiers `.po` valides et en-têtes complets ; chaque clé présente dans chaque langue, non vide, non « fuzzy », sans doublon ni clé obsolète ; mêmes variables que le texte source (`{nom}`, `{}`, `{n}`, accolades littérales) ; nombre de formes de pluriel conforme à la langue ; espaces de début et de fin conservés ; chargement des six catalogues et changement de langue |
| `cargo test -p halcyon-i18n` (unitaires) | pluriels, repli sur l'anglais puis sur le texte source, formats de nombres, textes différés, choix de langue (préférence, système, région voisine, langue indisponible, `C`/`POSIX`) |
| `cargo test -p halcyon-storage` | préférence conservée d'un lancement à l'autre ; anciens fichiers sans langue → automatique |
| `cargo test -p halcyon-app` | catalogues intégrés au binaire ; lecture de la langue de l'installateur (absente, valide, indisponible) |
| `python3 tools/i18n/i18n.py lint` | aucun texte visible hors traduction |

La compilation du launcher demande beaucoup de mémoire (interface générée d'environ 40 Mo de
Rust) : sur une machine de 8 Go, compiler avec `-j1` ou ajouter de l'espace d'échange.

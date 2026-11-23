# Halcyon 1.0.0 — Version stable

La 1.0.0 est la première version du canal **Stable**. Elle reprend tout ce que les préversions
1.0.0-alpha.1 à alpha.8 ont apporté et en retire trois fonctions peu utilisées. Vos données
sont conservées telles quelles : aucune migration de base, une base 1.0.0 reste lisible par
la 1.0.0-alpha.8.

Guide : [`GUIDE.md`](GUIDE.md) · design : [`DESIGN-SYSTEM.md`](DESIGN-SYSTEM.md) · historique
détaillé : [`CHANGELOG.md`](../CHANGELOG.md) · notes des préversions :
[`NOTES-1.0.0-alpha.1.md`](NOTES-1.0.0-alpha.1.md), [alpha.6](NOTES-1.0.0-alpha.6.md),
[alpha.7](NOTES-1.0.0-alpha.7.md), [alpha.8](NOTES-1.0.0-alpha.8.md).

## Ce qui change pour vous

### Trois fonctions retirées
- **Jeu à distance** : Halcyon ne pilote plus Sunshine et Moonlight, et la fiche n'a plus de
  bouton *Publier dans Sunshine…*. Les jeux déjà publiés restent dans Sunshine ; Moonlight et
  Sunshine s'utilisent directement.
- **Extensions** : plus de page Extensions ni d'installation de fichiers `.halcyonplugin`. Un
  dossier `plugins` laissé dans les données de Halcyon est ignoré (vous pouvez le supprimer).
- **Page Manettes** : la manette pilote toujours tout Halcyon et le mode console, avec la
  disposition standard (A valider, B retour, Y recherche ou retourner le boîtier, X favori,
  Select menu du jeu, LB / RB vue ou onglet, R3 cocher ou changer de vue, L3 inspecter). Si vous aviez créé un profil de touches dans une préversion, il reste
  appliqué.

### Trois écrans remis au propre
- **Sauvegardes** : le nombre de jeux suivis en grand, puis les jeux en rangées ; une perche
  orange signale ceux dont la dernière sauvegarde vient d'un autre PC. Dépôt, copies
  supplémentaires et conservation sont regroupés plus bas, dans une colonne de lecture.
- **Journal** : recherche et catégories dans un index à gauche, chaque jour sous un filet.
- **Fiche d'un jeu** : les onglets Statistiques, Sauvegardes, Mods, Lancement et Captures
  suivent la même mise en page que le reste de Halcyon (rubriques, rangées ouvertes, temps
  de jeu en grand).

### Mise à jour
- **À installer une fois à la main** depuis une préversion 1.0.0-alpha : la 1.0.0 est signée
  avec une nouvelle clé, que les préversions ne connaissent pas (elles la signalent sans
  l'installer). Vos données sont conservées.
- Ensuite, les mises à jour automatiques reprennent, sur le canal Stable comme sur Beta.

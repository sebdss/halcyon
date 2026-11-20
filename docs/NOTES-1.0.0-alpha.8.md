# Halcyon 1.0.0-alpha.8 — Boîtiers en vraie 3D

Rien n'est retiré : toutes les fonctions de la 1.0.0-alpha.7 restent. Aucune migration de base ;
un réglage s'ajoute (`console_3d`, activé par défaut).

Guide : [`GUIDE.md`](GUIDE.md) (section 6) · design : [`DESIGN-SYSTEM.md`](DESIGN-SYSTEM.md) ·
mesures : [`PERFORMANCES.md`](PERFORMANCES.md) · tests : [`TESTS-1.0.0-alpha.7.md`](TESTS-1.0.0-alpha.7.md)
(section « Vraie 3D »).

## Ce qui change pour vous

- L'étagère et le musée du mode console sont dessinés en **vraie 3D** : perspective, lumière,
  plastique qui brille, reflets des boîtiers sur la planche, vraie rotation quand un boîtier se
  tourne ou se retourne.
- **Inspecter** (bouton **L3** ou touche `I`) : le boîtier choisi seul, en grand. Les flèches
  le tournent (ou la souris en glissant), **A** l'ouvre — le couvercle pivote sur sa charnière
  et le disque apparaît —, **Y** le retourne, **LB / RB** (ou la molette) zooment, **B** ferme.
  Le bouton se change dans *Outils › Manettes*.
- Tout passe par OpenGL, déjà utilisé par Halcyon pour son affichage : aucune bibliothèque
  lourde ajoutée, et la scène n'est redessinée que si elle change.
- Sans OpenGL 3 (rendu logiciel), en mode Performance ou si vous coupez
  *Paramètres › Comportement › Boîtiers en 3D*, l'affichage 2,5D de la 1.0.0-alpha.7 prend le
  relais, à l'identique.

## Non vérifié faute de matériel
- Fluidité avec une vraie carte graphique (la 3D a été vérifiée sur un OpenGL émulé par le
  processeur, qui ne dit rien de la vitesse réelle), sous Windows, et à la manette réelle (Y,
  L3) : les mêmes chemins ont été vérifiés au clavier.

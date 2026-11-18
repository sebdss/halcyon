import React, {createContext, useContext} from 'react';
import {Img, staticFile} from 'remotion';

/**
 * Langue du film : textes à l'écran et captures (public/captures/ en français,
 * public/captures-en/ en anglais, mêmes noms de fichiers et mêmes cadrages).
 * Le montage, les temps et la bande-son sont communs aux deux langues.
 */
export type Lang = 'fr' | 'en';

const fr = {
  identity: ["Immobile au-dessus de l'eau,", 'puis un geste net.'],
  discoverTitle: ['Tous', 'vos', 'jeux.'],
  discoverSub: 'Une seule bibliothèque.',
  resumeLabel: 'Reprendre en un geste.',
  sourcesKicker: 'Toutes vos sources, une bibliothèque',
  sources: ['Steam', 'Epic Games', 'GOG', 'Heroic', 'Legendary', 'Vos dossiers', 'Émulateurs'],
  viewKicker: (i: number) => `Bibliothèque · vue ${i} sur 5`,
  views: ['Grille', 'Liste', 'Carrousel', 'Immersive'],
  detailKicker: 'Fiche de jeu',
  detailTitle: 'Chaque jeu, ses vrais chiffres.',
  detailPlaytime: 'Temps de jeu réel, avec sa source',
  detailTabs: 'Succès · Sauvegardes · Mods · Lancement',
  consoleKicker: 'Mode console',
  consoleTitle: 'Du bureau au canapé.',
  tools: [
    {title: 'Mises à jour', line: 'Ce qui attend chez Steam, Epic et GOG, avec la progression réelle.'},
    {title: 'Que jouer ?', line: 'Des suggestions calculées sur votre PC, chacune avec sa raison.'},
    {title: 'Statistiques', line: 'Votre temps de jeu, exportable en CSV ou en JSON.'},
    {title: 'Apparence', line: 'Thèmes, accent, densité, mouvement, taille du texte.'},
  ],
  emuKicker: 'Émulateurs reconnus',
  emuMulti: 'Multi-systèmes',
  emuTitle: 'Vos ROM, rangées comme vos jeux PC.',
  immersion: ['Natif.', 'Léger.', 'Hors ligne.'],
  playTitle: ['Un seul geste', 'pour jouer.'],
  play: 'Jouer',
  signatureInfo: ['Windows', 'Linux', 'Gratuit et open source', 'Halcyon 1.0 en préversion'],
  signatureLine: 'Tous vos jeux. Un seul geste pour jouer.',
  download: 'Télécharger Halcyon',
};

export type Strings = typeof fr;

const en: Strings = {
  identity: ['Perfectly still above the water,', 'then one clean move.'],
  discoverTitle: ['All', 'your', 'games.'],
  discoverSub: 'One single library.',
  resumeLabel: 'Pick up in one move.',
  sourcesKicker: 'All your sources, one library',
  sources: ['Steam', 'Epic Games', 'GOG', 'Heroic', 'Legendary', 'Your folders', 'Emulators'],
  viewKicker: (i: number) => `Library · view ${i} of 5`,
  views: ['Grid', 'List', 'Carousel', 'Immersive'],
  detailKicker: 'Game page',
  detailTitle: 'Every game, with its real numbers.',
  detailPlaytime: 'Real playtime, with its source',
  detailTabs: 'Achievements · Saves · Mods · Launch',
  consoleKicker: 'Console mode',
  consoleTitle: 'From desk to couch.',
  tools: [
    {title: 'Updates', line: "What's pending on Steam, Epic and GOG, with the real progress."},
    {title: 'What to play?', line: 'Suggestions computed on your PC, each with its reason.'},
    {title: 'Statistics', line: 'Your playtime, exportable as CSV or JSON.'},
    {title: 'Appearance', line: 'Themes, accent, density, motion, text size.'},
  ],
  emuKicker: 'Recognized emulators',
  emuMulti: 'Multi-system',
  emuTitle: 'Your ROMs, organized like your PC games.',
  immersion: ['Native.', 'Light.', 'Offline.'],
  playTitle: ['One move', 'to play.'],
  play: 'Play',
  signatureInfo: ['Windows', 'Linux', 'Free and open source', 'Halcyon 1.0 in preview'],
  signatureLine: 'All your games. One move to play.',
  download: 'Download Halcyon',
};

const STRINGS: Record<Lang, Strings> = {fr, en};
const LangContext = createContext<Lang>('fr');

export const LangProvider: React.FC<{lang: Lang; children: React.ReactNode}> = ({lang, children}) => (
  <LangContext.Provider value={lang}>{children}</LangContext.Provider>
);

export const useLang = (): Lang => useContext(LangContext);
export const useT = (): Strings => STRINGS[useContext(LangContext)];

/** Dossier des captures (sous public/) pour la langue du film. */
export const useCaptures = (): string => (useContext(LangContext) === 'en' ? 'captures-en' : 'captures');

/** Une capture de l'interface (public/captures/ ou public/captures-en/) dans la langue du film. */
export const Capture: React.FC<{name: string; style?: React.CSSProperties}> = ({name, style}) => {
  const dir = useCaptures();
  return <Img src={staticFile(`${dir}/${name}.jpg`)} style={style} />;
};

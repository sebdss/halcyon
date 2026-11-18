import {continueRender, delayRender, staticFile} from 'remotion';

// Archivo (SIL OFL) : les fichiers mêmes du launcher.
const FACES: [string, string, number][] = [
  ['Archivo', 'Archivo-Regular.ttf', 400],
  ['Archivo', 'Archivo-Medium.ttf', 500],
  ['Archivo', 'Archivo-SemiBold.ttf', 600],
  ['Archivo', 'Archivo-Bold.ttf', 700],
  ['Archivo Expanded', 'ArchivoExpanded-SemiBold.ttf', 600],
  ['Archivo Expanded', 'ArchivoExpanded-Bold.ttf', 700],
  ['Archivo Expanded', 'ArchivoExpanded-ExtraBold.ttf', 800],
];

let started = false;
export const loadFonts = () => {
  if (started || typeof document === 'undefined') return;
  started = true;
  const handle = delayRender('Polices Archivo');
  Promise.all(
    FACES.map(([family, file, weight]) => {
      const face = new FontFace(family, `url(${staticFile(`fonts/${file}`)})`, {weight: String(weight)});
      return face.load().then((loaded) => document.fonts.add(loaded));
    }),
  )
    .then(() => continueRender(handle))
    .catch((e) => {
      console.error(e);
      continueRender(handle);
    });
};

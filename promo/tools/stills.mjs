// Vérification rapide : rend quelques images clés sans rendre la vidéo.
// node tools/stills.mjs <composition> <dossier> <image> [<image>…]
import {bundle} from '@remotion/bundler';
import {openBrowser, renderStill, selectComposition} from '@remotion/renderer';
import path from 'node:path';

const [comp, dir, ...frames] = process.argv.slice(2);
const serveUrl = await bundle({entryPoint: path.resolve('src/index.ts'), publicDir: path.resolve('public')});
const browser = await openBrowser('chrome', {browserExecutable: process.env.REMOTION_CHROME || '/usr/bin/google-chrome'});
const composition = await selectComposition({serveUrl, id: comp, puppeteerInstance: browser, inputProps: {audio: 'aucun'}});
for (const fr of frames) {
  await renderStill({composition, serveUrl, output: path.join(dir, `${comp}_${fr}.jpeg`), frame: Number(fr), imageFormat: 'jpeg', jpegQuality: 90, puppeteerInstance: browser, inputProps: {audio: 'aucun'}});
  console.log('image', fr);
}
await browser.close({silent: true});

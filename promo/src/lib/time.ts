import {Easing, interpolate} from 'remotion';
import timeline from '../timeline.json';

export const FPS = timeline.fps;
export const BEAT = Math.round((60 / timeline.bpm) * FPS); // 30 images
export const BAR = BEAT * 4; // 120 images
export const sec = (s: number) => Math.round(s * FPS);

export type SceneId = (typeof timeline.scenes)[number]['id'];
export const scene = (id: SceneId) => {
  const s = timeline.scenes.find((x) => x.id === id)!;
  return {from: sec(s.from), duration: sec(s.to) - sec(s.from)};
};

// Courbes : sortie franche et arrêt doux, sans rebond (« rien ne flotte ni ne rebondit »).
export const ease = {
  out: Easing.bezier(0.16, 1, 0.3, 1),
  inOut: Easing.bezier(0.65, 0, 0.35, 1),
  in: Easing.bezier(0.55, 0, 1, 0.45),
  snap: Easing.bezier(0.2, 0, 0, 1),
};

/** Interpolation bornée avec courbe : tw(f, [a, b], [x, y], ease.out). */
export const tw = (
  f: number,
  range: [number, number],
  out: [number, number],
  easing: (t: number) => number = ease.out,
) => interpolate(f, range, out, {easing, extrapolateLeft: 'clamp', extrapolateRight: 'clamp'});

/** Valeur multi-points bornée. */
export const keys = (f: number, input: number[], output: number[], easing = ease.inOut) =>
  interpolate(f, input, output, {easing, extrapolateLeft: 'clamp', extrapolateRight: 'clamp'});

import {useVideoConfig} from 'remotion';

/**
 * Repère de mise en page. Les scènes sont dessinées dans une scène de 1920 × 1080 (paysage) ;
 * en 9:16, `v` est vrai et les scènes adaptent leurs positions clés (voir README).
 */
export const useLayout = () => {
  const {width, height} = useVideoConfig();
  const v = height > width;
  // unité : 1 = 1 px d'une sortie 1080p de référence
  const u = v ? width / 1080 : height / 1080;
  return {W: width, H: height, v, u, cx: width / 2, cy: height / 2};
};

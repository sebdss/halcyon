import React from 'react';
import {AbsoluteFill, useCurrentFrame} from 'remotion';
import {Panel} from '../components/Shot';
import {Reveal, display} from '../components/Type';
import {useLayout} from '../lib/layout';
import {C} from '../lib/theme';
import {BEAT, ease, keys, tw} from '../lib/time';

const WALL = [
  ['accueil', 'bibliotheque', 'fiche', 'liste'],
  ['immersive', 'console-3', 'que-jouer', 'carrousel'],
  ['statistiques', 'grille-m', 'mises-a-jour', 'console-5'],
  ['apparence', 'console-1', 'emulateurs', 'accueil'],
];

/**
 * 40–44 s · L'immersion. Un mur des vrais écrans de Halcyon, vu en plongée ; la caméra le
 * survole pendant que trois mots tombent sur les temps forts.
 */
export const Immersion: React.FC = () => {
  const f = useCurrentFrame();
  const {W, H, u, v} = useLayout();
  const pw = 640 * u;
  const ph = pw * 0.625;
  const gap = 46 * u;
  const pan = keys(f, [0, 240], [0, 1], (t) => t);
  const enter = tw(f, [0, 40], [0, 1], ease.out);
  const dim = keys(f, [40, 70], [0, 0.5]);
  const exit = tw(f, [228, 240], [0, 1], ease.in);
  const words = ['Natif.', 'Léger.', 'Hors ligne.'];
  return (
    <AbsoluteFill style={{background: C.console, overflow: 'hidden', perspective: 1600}}>
      <div
        style={{
          position: 'absolute',
          left: W / 2,
          top: H / 2,
          transformStyle: 'preserve-3d',
          transform: `translateZ(${-260 * u + enter * 120 * u}px) rotateX(${50 - enter * 6}deg) rotateZ(${-24 + pan * 6}deg) translate(${(-0.62 + pan * 0.32) * 4 * (pw + gap)}px, ${(-0.75 + pan * 0.25) * 4 * (ph + gap)}px)`,
          opacity: 1 - exit,
        }}
      >
        {WALL.map((row, r) =>
          row.map((src, c) => (
            <div
              key={`${r}-${c}`}
              style={{
                position: 'absolute',
                left: c * (pw + gap) + (r % 2) * 120 * u,
                top: r * (ph + gap),
                transform: `translateZ(${((r + c) % 3) * 40 * u * (1 - enter * 0.4)}px)`,
              }}
            >
              <Panel src={src} width={pw} radius={6 * u} />
            </div>
          )),
        )}
      </div>
      <AbsoluteFill style={{background: C.console, opacity: dim}} />
      <AbsoluteFill style={{justifyContent: 'center', paddingLeft: (v ? 80 : 170) * u, opacity: 1 - exit}}>
        {words.map((w, i) => (
          <Reveal key={w} text={w} f={f} at={(i + 1) * 2 * BEAT} dur={20} style={display((v ? 120 : 150) * u, i === 2 ? C.text : C.text, 800)} />
        ))}
      </AbsoluteFill>
    </AbsoluteFill>
  );
};

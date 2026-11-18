import React from 'react';
import {AbsoluteFill, useCurrentFrame} from 'remotion';
import {Panel} from '../components/Shot';
import {Reveal, body, display} from '../components/Type';
import {useLayout} from '../lib/layout';
import {C} from '../lib/theme';
import {ease, tw} from '../lib/time';
import {useT} from '../lib/lang';

const ITEMS = [
  {src: 'mises-a-jour'},
  {src: 'que-jouer'},
  {src: 'statistiques'},
  {src: 'apparence'},
];
const CROP = {x: 212, y: 0, w: 1228, h: 900};

/**
 * 34–38 s · Les outils. Un travelling latéral le long d'une rangée de panneaux réels ; chaque
 * panneau se pose sur son temps et la caméra le suit.
 */
export const Tools: React.FC = () => {
  const f = useCurrentFrame();
  const {W, H, u, v} = useLayout();
  const tr = useT();
  const pw = (v ? 860 : 1060) * u; // largeur d'un panneau
  const gap = 130 * u;
  // position de caméra : avance d'un panneau à chaque mesure, avec arrivée douce
  let idx = 0;
  for (let i = 1; i < ITEMS.length; i++) idx += tw(f, [i * 60 - 14, i * 60 + 16], [0, 1], ease.inOut);
  const drift = tw(f, [0, 240], [0, 1], (t) => t) * 60 * u;
  const camX = idx * (pw + gap) + drift;
  const exit = tw(f, [226, 240], [0, 1], ease.in);
  return (
    <AbsoluteFill style={{background: C.console, overflow: 'hidden', perspective: 2000}}>
      <div
        style={{
          position: 'absolute',
          left: W / 2 - pw / 2 - camX,
          top: H / 2 - (pw * 900) / 1228 / 2 + 70 * u,
          transformStyle: 'preserve-3d',
          transform: `rotateY(-9deg) rotateX(3deg)`,
          opacity: 1 - exit,
        }}
      >
        {ITEMS.map((it, i) => {
          const p = tw(f, [i * 60 - 10, i * 60 + 18], [0, 1], ease.out);
          const focus = Math.max(0, 1 - Math.abs(idx - i));
          return (
            <div
              key={it.src}
              style={{
                position: 'absolute',
                left: i * (pw + gap),
                top: 0,
                opacity: p * (0.42 + 0.58 * focus),
                transform: `translate3d(0, ${(1 - p) * 80 * u}px, ${(focus - 1) * 140 * u}px)`,
              }}
            >
              <div style={{position: 'absolute', top: -118 * u, left: 0, width: pw}}>
                <Reveal text={tr.tools[i].title} f={f} at={i * 60} stagger={3} dur={18} style={display(46 * u, C.text, 800)} />
                <div style={{...body(24 * u, C.muted, 500), marginTop: 10 * u, opacity: tw(f, [i * 60 + 8, i * 60 + 26], [0, 1])}}>{tr.tools[i].line}</div>
              </div>
              <Panel src={it.src} crop={CROP} width={pw} radius={6 * u} />
            </div>
          );
        })}
      </div>
    </AbsoluteFill>
  );
};

import React from 'react';
import {AbsoluteFill, useCurrentFrame} from 'remotion';
import {Base, Stage} from '../components/Layer';
import {Lower} from '../components/Lower';
import {useLayout} from '../lib/layout';
import {C} from '../lib/theme';
import {ease, keys, tw} from '../lib/time';
import {useT} from '../lib/lang';

const VIEWS = [
  {src: 'bibliotheque', name: 'Grille'},
  {src: 'liste', name: 'Liste'},
  {src: 'carrousel', name: 'Carrousel'},
  {src: 'immersive', name: 'Immersive'},
];

/**
 * 22–26 s · Les vues de la bibliothèque, une par mesure… une par temps fort : chaque passage
 * est un volet dont le bord est la perche bleue du launcher.
 */
export const Views: React.FC = () => {
  const f = useCurrentFrame();
  const {W, H, u, v} = useLayout();
  const tr = useT();
  const zoom = keys(f, [0, 240], [(W / 1440) * (v ? 1.9 : 1.0), (W / 1440) * (v ? 2.0 : 1.07)]);
  const focus: [number, number] = v ? [900, 450] : [720, 450];
  return (
    <AbsoluteFill style={{background: C.console, overflow: 'hidden'}}>
      {VIEWS.map((view, i) => {
        const start = i * 60;
        const p = i === 0 ? tw(f, [0, 16], [0, 1], ease.inOut) : tw(f, [start - 8, start + 8], [0, 1], ease.inOut);
        if (p <= 0) return null;
        const edge = W * p;
        return (
          <AbsoluteFill key={view.src} style={{clipPath: `inset(0 ${100 - p * 100}% 0 0)`}}>
            <Stage focus={focus} zoom={zoom} at={[W / 2, H / 2]}>
              <Base src={view.src} />
            </Stage>
            {p < 1 ? <div style={{position: 'absolute', left: edge - 3 * u, top: 0, bottom: 0, width: 5 * u, background: C.accent}} /> : null}
          </AbsoluteFill>
        );
      })}
      {VIEWS.map((view, i) => (
        <Lower key={view.name} f={f} at={i * 60 + 2} out={i < 3 ? i * 60 + 52 : 228} kicker={tr.viewKicker(i + 1)} title={tr.views[i]} />
      ))}
    </AbsoluteFill>
  );
};

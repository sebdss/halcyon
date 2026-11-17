import React from 'react';
import {AbsoluteFill, useCurrentFrame} from 'remotion';
import {Base, Stage} from '../components/Layer';
import {Reveal, body, display} from '../components/Type';
import {useLayout} from '../lib/layout';
import {C} from '../lib/theme';
import {BEAT, ease, keys, tw} from '../lib/time';

/**
 * 30–34 s · Le mode console. Carton de titre, puis l'écran de télévision monte ; la sélection
 * avance d'un jeu par temps, exactement comme à la manette (sept captures successives).
 */
export const Console: React.FC = () => {
  const f = useCurrentFrame();
  const {W, H, u, v} = useLayout();
  const rise = tw(f, [38, 72], [0, 1], ease.inOut);
  const zoom = keys(f, [38, 240], [(W / 1440) * (v ? 2.1 : 1.0), (W / 1440) * (v ? 2.2 : 1.06)]);
  const focus: [number, number] = v ? [380, 450] : [720, 450];
  // état courant : 0 au départ, puis un jeu de plus à chaque temps depuis 31 s
  const states = [0, 1, 2, 3, 4, 5, 6];
  return (
    <AbsoluteFill style={{background: '#000', overflow: 'hidden'}}>
      <AbsoluteFill style={{clipPath: `inset(${(1 - rise) * 100}% 0 0 0)`}}>
        {states.map((s) => {
          const t0 = s === 0 ? 0 : 60 + (s - 1) * BEAT;
          const p = s === 0 ? 1 : tw(f, [t0, t0 + 10], [0, 1], ease.out);
          return p > 0 ? (
            <Stage key={s} focus={focus} zoom={zoom * (1 + (1 - rise) * 0.08)} at={[W / 2, H / 2]} opacity={p}>
              <Base src={`console-${s}`} />
            </Stage>
          ) : null;
        })}
      </AbsoluteFill>
      {/* carton */}
      <AbsoluteFill
        style={{
          alignItems: 'center',
          justifyContent: 'center',
          flexDirection: 'column',
          transform: `translateY(${-rise * 380 * u}px)`,
          opacity: 1 - tw(f, [44, 66], [0, 1]),
        }}
      >
        <div style={{...body(26 * u, C.muted, 600), marginBottom: 18 * u, opacity: tw(f, [0, 14], [0, 1])}}>Mode console</div>
        <Reveal text="Du bureau au canapé." f={f} at={2} stagger={4} dur={20} style={{...display((v ? 76 : 110) * u, C.text, 800), justifyContent: 'center'}} />
      </AbsoluteFill>
    </AbsoluteFill>
  );
};

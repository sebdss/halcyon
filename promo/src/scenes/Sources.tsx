import React from 'react';
import {AbsoluteFill, useCurrentFrame} from 'remotion';
import {Base, Hole, Layer, Stage} from '../components/Layer';
import {body, display} from '../components/Type';
import {useLayout} from '../lib/layout';
import {C} from '../lib/theme';
import {BEAT, ease, keys, tw} from '../lib/time';
import {useT} from '../lib/lang';

// jaquettes de la grille (capture « grille-m », repère de l'interface)
const COVERS = [
  [480, 198], [716, 198], [951, 198], [1186, 198],
  [480, 597], [716, 597], [951, 597], [1186, 597],
];

/**
 * 18–22 s · Les sources. Les noms défilent comme une bobine, un par temps ; à droite, la
 * bibliothèque réelle se remplit jaquette par jaquette sur les mêmes temps.
 */
export const Sources: React.FC = () => {
  const f = useCurrentFrame();
  const {W, H, u, v} = useLayout();
  const tr = useT();
  const NAMES = tr.sources;
  // index continu de la bobine : saut net sur chaque temps
  let cur = 0;
  for (let i = 1; i < NAMES.length; i++) cur += tw(f, [i * BEAT - 4, i * BEAT + 10], [0, 1], ease.out);
  const enter = tw(f, [0, 30], [0, 1], ease.out);
  const exit = tw(f, [222, 240], [0, 1], ease.in);
  const zoom = keys(f, [0, 240], [0.74 * u, 0.8 * u]);

  return (
    <AbsoluteFill style={{background: C.console, overflow: 'hidden'}}>
      <Stage
        focus={[720, 450]}
        zoom={zoom}
        at={[v ? W / 2 : W * 0.735 - exit * 200 * u, v ? H * 0.68 : H / 2 + 50 * u]}
        ry={-18 + 6 * enter}
        rx={4}
        tz={-80 * (1 - enter)}
        perspective={1800}
        opacity={enter * (1 - exit)}
      >
        <Base src="grille-m" />
        {COVERS.map(([x, y], i) => {
          const p = tw(f, [i * BEAT, i * BEAT + 22], [0, 1], ease.out);
          return (
            <React.Fragment key={i}>
              <Hole x={x - 2} y={y - 2} w={218} h={y > 500 ? 305 : 365} />
              <Layer src="grille-m" x={x} y={y} w={214} h={y > 500 ? 303 : 362} z={(1 - p) * 220} dy={(1 - p) * -30} opacity={p} />
            </React.Fragment>
          );
        })}
      </Stage>

      {/* bobine */}
      <div style={{position: 'absolute', left: (v ? 80 : 150) * u, top: v ? 300 * u : H / 2 + 40 * u, opacity: 1 - exit}}>
        <div style={{...body(24 * u, C.muted, 600), position: 'absolute', top: -300 * u, whiteSpace: 'nowrap', opacity: enter}}>
          {tr.sourcesKicker}
        </div>
        {NAMES.map((name, i) => {
          const d = i - cur;
          const a = d > 0.5 ? Math.max(0, 1 - (d - 0.5) * 1.6) : d < -2.2 ? Math.max(0, 1 - (-2.2 - d) * 1.5) * 0.25 : Math.max(0.25, 1 - Math.abs(d) * 0.38);
          const active = Math.max(0, 1 - Math.abs(d) * 1.6);
          return (
            <div
              key={name}
              style={{
                position: 'absolute',
                top: d * 96 * u,
                left: 0,
                whiteSpace: 'nowrap',
                opacity: a * (i * BEAT - 6 <= f ? 1 : 0),
                transform: `translateX(${(1 - active) * 0 + (d > 0 ? 40 * u * d : 0)}px)`,
                ...display(76 * u, active > 0.5 ? C.text : C.dim, 800),
              }}
            >
              <span style={{position: 'absolute', left: -46 * u, top: 40 * u, width: 22 * u * active, height: 5 * u, background: C.accent}} />
              {name}
            </div>
          );
        })}
      </div>
    </AbsoluteFill>
  );
};

import React from 'react';
import {AbsoluteFill, useCurrentFrame} from 'remotion';
import {Callout, Lower} from '../components/Lower';
import {Base, Stage} from '../components/Layer';
import {useLayout} from '../lib/layout';
import {C} from '../lib/theme';
import {ease, keys, tw} from '../lib/time';
import {useT} from '../lib/lang';

/**
 * 26–30 s · La fiche d'un jeu. Plan serré sur la jaquette qui chevauche la bande, recul sur
 * la fiche entière ; deux repères suivent l'interface pendant le mouvement.
 */
export const Detail: React.FC = () => {
  const f = useCurrentFrame();
  const {W, H, u, v} = useLayout();
  const tr = useT();
  const kf = [0, 110, 240];
  const zoom = keys(f, kf, [2.7 * u, 1.42 * u, 1.5 * u], ease.inOut);
  const fx = keys(f, kf, [346, v ? 640 : 800, v ? 660 : 820]);
  const fy = keys(f, kf, [330, 430, 440]);
  const ry = keys(f, kf, [12, 0, -2]);
  const rz = keys(f, kf, [-2.5, 0, 0]);
  const enter = tw(f, [0, 14], [0, 1]);
  const exit = tw(f, [226, 240], [0, 1], ease.in);
  return (
    <AbsoluteFill style={{background: C.console, overflow: 'hidden'}}>
      <Stage focus={[fx, fy]} zoom={zoom} at={[W / 2 - exit * 300 * u, H / 2]} ry={ry} rz={rz} opacity={enter * (1 - exit)}>
        <Base src="fiche" />
        <Callout f={f} at={60} x={1192} y={500} dx={-60} dy={-140} zoom={zoom} text={tr.detailPlaytime} align="right" />
        <Callout f={f} at={120} x={560} y={578} dx={90} dy={110} zoom={zoom} text={tr.detailTabs} />
      </Stage>
      <Lower f={f} at={8} out={222} top={72} kicker={tr.detailKicker} title={tr.detailTitle} />
    </AbsoluteFill>
  );
};

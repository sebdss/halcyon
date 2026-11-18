import React from 'react';
import {AbsoluteFill, useCurrentFrame} from 'remotion';
import {Base, Hole, Layer, Stage} from '../components/Layer';
import {Reveal, display} from '../components/Type';
import {useLayout} from '../lib/layout';
import {C} from '../lib/theme';
import {BEAT, ease, keys, tw} from '../lib/time';
import {useT} from '../lib/lang';

/**
 * 10–18 s · La découverte. Le titre se compose sur les temps, puis l'accueil réel de Halcyon
 * entre en 3D : la colonne de navigation et l'étagère de jaquettes se détachent puis se
 * reposent. La caméra s'approche de « Reprendre » et de Jouer, puis plonge dans l'illustration.
 */
export const Discover: React.FC = () => {
  const f = useCurrentFrame();
  const {W, H, u, v} = useLayout();
  const tr = useT();

  // titre
  const titleOut = tw(f, [132, 186], [0, 1], ease.in);
  const perch = tw(f, [130, 160], [0, 1], ease.out);

  // entrée de l'interface (12,6 s → 15 s)
  const p = tw(f, [150, 300], [0, 1], ease.out);
  const sep = 1 - tw(f, [190, 320], [0, 1], ease.inOut); // séparation des calques
  const fitZoom = (v ? 1.0 : 1.08) * (W / 1440) * (v ? 1 : 0.94);

  // caméra : plan d'ensemble → « Reprendre » (15 s) → plongée dans l'illustration (17,4 s)
  const kf = [300, 380, 432, 480];
  const zoom = keys(f, kf, [fitZoom, 2.05 * u, 2.25 * u, 9 * u], ease.inOut);
  const fx = keys(f, kf, [720, 360, 380, 1090]);
  const fy = keys(f, kf, [450, 395, 395, 250]);
  const atX = keys(f, kf, [W / 2, W * 0.42, W * 0.42, W / 2]);
  const atY = keys(f, kf, [H / 2 + 20 * u, H / 2, H / 2, H / 2]);
  const dive = tw(f, [432, 480], [0, 1], ease.in);
  const ring = tw(f, [330, 342], [0, 1], ease.out);
  const label = f - 336;

  return (
    <AbsoluteFill style={{background: C.console, overflow: 'hidden'}}>
      {/* l'interface */}
      <Stage
        focus={[fx, fy]}
        zoom={zoom}
        at={[atX, atY + (1 - p) * 760 * u]}
        rx={(1 - p) * 42}
        rz={(1 - p) * -9}
        ry={(1 - p) * 6}
        opacity={tw(f, [150, 175], [0, 1]) * (1 - tw(f, [468, 480], [0, 1]))}
        blur={dive * 6}
      >
        <Base src="accueil" />
        {/* creux laissés par les panneaux qui se soulèvent */}
        <Hole x={0} y={56} w={212} h={844} color={C.rive} on={sep > 0.01} />
        <Hole x={252} y={570} w={990} h={270} on={sep > 0.01} />
        <Hole x={734} y={88} w={666} h={360} on={sep > 0.01} />
        <Layer src="accueil" x={0} y={0} w={212} h={900} z={90 * sep} dx={-40 * sep} />
        <Layer src="accueil" x={252} y={570} w={990} h={270} z={150 * sep} dy={40 * sep} />
        <Layer src="accueil" x={734} y={88} w={666} h={360} z={60 * sep} dx={30 * sep} />
        {/* anneau de focus (2 px, à 3 px du contrôle, comme dans le launcher) autour de Jouer */}
        <div
          style={{
            position: 'absolute',
            left: 249,
            top: 397,
            width: 162,
            height: 54,
            borderRadius: 8,
            border: `${1.3}px solid ${C.accent}`,
            opacity: ring * (1 - dive),
            transform: `scale(${1.08 - 0.08 * ring})`,
          }}
        />
        {/* repère attaché à l'interface : suit les mouvements de caméra */}
        {label > 0 ? (
          <div style={{position: 'absolute', left: 252, top: 470, opacity: 1 - dive}}>
            <div style={{width: tw(label, [0, 14], [0, 18]), height: 1.5, background: C.accent, marginBottom: 7}} />
            <Reveal text={tr.resumeLabel} f={label} at={4} stagger={4} dur={18} style={{...display(16, C.text, 700), whiteSpace: 'nowrap'}} />
          </div>
        ) : null}
      </Stage>

      {/* plongée : l'illustration envahit l'image, fondu vers la scène suivante */}
      <AbsoluteFill style={{background: C.console, opacity: tw(f, [462, 480], [0, 1], ease.in)}} />

      {/* titre : sur les temps 10 – 10,5 – 11 s, puis 12 s */}
      <AbsoluteFill
        style={{
          padding: v ? `${360 * u}px ${80 * u}px` : `${250 * u}px ${170 * u}px`,
          transform: `translateY(${-titleOut * 620 * u}px)`,
          opacity: 1 - tw(f, [160, 196], [0, 1]),
        }}
      >
        <div style={{display: 'flex', gap: `0 ${0.3 * 176 * u}px`, flexWrap: 'wrap'}}>
          {tr.discoverTitle.map((w, i) => (
            <Reveal key={w} text={w} f={f} at={i * BEAT} dur={20} style={display((v ? 150 : 176) * u, C.text, 800)} />
          ))}
        </div>
        <div style={{marginTop: 26 * u, display: 'flex', alignItems: 'baseline', gap: 22 * u, flexWrap: 'wrap'}}>
          <Reveal text={tr.discoverSub} f={f} at={4 * BEAT} stagger={5} dur={22} style={display((v ? 64 : 76) * u, C.muted, 700)} />
        </div>
        <div style={{marginTop: 22 * u, width: 120 * u * perch, height: 6 * u, background: C.accent}} />
      </AbsoluteFill>
    </AbsoluteFill>
  );
};

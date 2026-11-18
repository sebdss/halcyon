import React from 'react';
import {Reveal, body, display} from './Type';
import {C} from '../lib/theme';
import {ease, tw} from '../lib/time';
import {useLayout} from '../lib/layout';

/**
 * Bandeau de titre : posé sur la « rive » (teinte de la colonne du launcher), perche bleue,
 * surtitre discret et titre en Archivo Expanded. Entre par la gauche, sort par masque.
 */
export const Lower: React.FC<{f: number; at?: number; out?: number; kicker: string; title: string; bottom?: number; top?: number; left?: number}> = ({
  f,
  at = 0,
  out,
  kicker,
  title,
  bottom = 88,
  top,
  left = 0,
}) => {
  const {u} = useLayout();
  const p = tw(f, [at, at + 22], [0, 1], ease.out);
  const q = out === undefined ? 0 : tw(f, [out, out + 16], [0, 1], ease.in);
  return (
    <div
      style={{
        position: 'absolute',
        left: left * u,
        ...(top !== undefined ? {top: top * u} : {bottom: bottom * u}),
        clipPath: `inset(0 ${100 - 100 * p + 100 * q}% 0 0)`,
        background: C.rive,
        padding: `${26 * u}px ${44 * u}px ${30 * u}px ${56 * u}px`,
        boxShadow: `0 0 0 1px ${C.border}`,
      }}
    >
      <div style={{position: 'absolute', left: 28 * u, top: 32 * u, width: 4 * u, height: 26 * u, background: C.accent}} />
      <div style={{...body(22 * u, C.muted, 600), marginBottom: 8 * u}}>{kicker}</div>
      <Reveal text={title} f={f} at={at + 6} stagger={3} dur={20} style={{...display(46 * u, C.text, 800), whiteSpace: 'nowrap', flexWrap: 'nowrap'}} />
    </div>
  );
};

/** Repère accroché à un point de l'interface (dans le repère d'une Stage). */
export const Callout: React.FC<{f: number; at: number; x: number; y: number; dx: number; dy: number; zoom: number; text: string; align?: 'left' | 'right'}> = ({
  f,
  at,
  x,
  y,
  dx,
  dy,
  zoom,
  text,
  align = 'left',
}) => {
  const {u} = useLayout();
  const k = f - at;
  if (k < 0) return null;
  const s = u / zoom; // taille constante à l'écran
  const dot = tw(k, [0, 10], [0, 1], ease.out);
  const line = tw(k, [4, 20], [0, 1], ease.out);
  const len = Math.hypot(dx, dy);
  const ang = (Math.atan2(dy, dx) * 180) / Math.PI;
  return (
    <div style={{position: 'absolute', left: x, top: y}}>
      <div
        style={{
          position: 'absolute',
          left: -7 * s,
          top: -7 * s,
          width: 14 * s,
          height: 14 * s,
          borderRadius: 2 * s,
          border: `${2 * s}px solid ${C.accent}`,
          transform: `scale(${dot})`,
        }}
      />
      <div style={{position: 'absolute', left: 0, top: -1 * s, width: len * line, height: 2 * s, background: C.accent, transformOrigin: '0 50%', transform: `rotate(${ang}deg)`}} />
      <div
        style={{
          position: 'absolute',
          left: dx + (align === 'left' ? 10 * s : -10 * s),
          top: dy - 22 * s,
          transform: align === 'right' ? 'translateX(-100%)' : undefined,
          background: C.rive,
          padding: `${10 * s}px ${16 * s}px`,
          borderRadius: 4 * s,
          boxShadow: `0 0 0 ${1 * s}px ${C.borderStrong}`,
          opacity: tw(k, [14, 26], [0, 1]),
          whiteSpace: 'nowrap',
        }}
      >
        <Reveal text={text} f={k} at={14} stagger={2} dur={16} style={{...display(22 * s, C.text, 700), flexWrap: 'nowrap'}} />
      </div>
    </div>
  );
};

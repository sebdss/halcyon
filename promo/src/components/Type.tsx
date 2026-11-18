import React from 'react';
import {ease, tw} from '../lib/time';
import {C, F} from '../lib/theme';

/**
 * Texte révélé par masque : chaque mot (ou lettre) monte depuis sous sa ligne de base.
 * `f` image locale, `at` début, `stagger` images entre deux unités, `out` image de sortie.
 */
export const Reveal: React.FC<{
  text: string;
  f: number;
  at?: number;
  stagger?: number;
  dur?: number;
  by?: 'word' | 'char';
  out?: number;
  outDur?: number;
  style?: React.CSSProperties;
  unitStyle?: (i: number) => React.CSSProperties;
}> = ({text, f, at = 0, stagger = 4, dur = 22, by = 'word', out, outDur = 14, style, unitStyle}) => {
  const units = by === 'word' ? text.split(' ') : Array.from(text);
  return (
    <div style={{display: 'flex', flexWrap: 'wrap', ...style}}>
      {units.map((u, i) => {
        const p = tw(f, [at + i * stagger, at + i * stagger + dur], [0, 1], ease.out);
        const q = out === undefined ? 0 : tw(f, [out + i * (stagger / 2), out + i * (stagger / 2) + outDur], [0, 1], ease.in);
        const y = (1 - p) * 105 - q * 105;
        return (
          <span
            key={i}
            style={{
              display: 'inline-block',
              overflow: 'hidden',
              paddingBottom: '0.12em',
              marginBottom: '-0.12em',
              marginRight: by === 'word' && i < units.length - 1 ? '0.28em' : 0,
              whiteSpace: 'pre',
            }}
          >
            <span style={{display: 'inline-block', transform: `translateY(${y}%)`, ...(unitStyle ? unitStyle(i) : {})}}>{u}</span>
          </span>
        );
      })}
    </div>
  );
};

export const display = (size: number, color = C.text, weight = 800): React.CSSProperties => ({
  fontFamily: F.display,
  fontWeight: weight,
  fontSize: size,
  lineHeight: 1.04,
  color,
});

export const body = (size: number, color = C.muted, weight = 500): React.CSSProperties => ({
  fontFamily: F.text,
  fontWeight: weight,
  fontSize: size,
  lineHeight: 1.4,
  color,
});

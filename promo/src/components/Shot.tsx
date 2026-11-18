import React from 'react';
import {Capture} from '../lib/lang';

/** Capture réelle de Halcyon (2880 × 1800, interface logique 1440 × 900). */
export const UI_W = 1440;
export const UI_H = 900;

type Rect = {x: number; y: number; w: number; h: number};

/**
 * Caméra sur une capture : le point `focus` (coordonnées de l'interface) est placé à `at`
 * (coordonnées de l'écran) avec `zoom` pixels d'écran par pixel d'interface.
 */
export const Camera: React.FC<{
  src: string;
  focus: [number, number];
  zoom: number;
  at: [number, number];
  opacity?: number;
  children?: React.ReactNode;
}> = ({src, focus, zoom, at, opacity = 1, children}) => (
  <div
    style={{
      position: 'absolute',
      left: at[0] - focus[0] * zoom,
      top: at[1] - focus[1] * zoom,
      width: UI_W * zoom,
      height: UI_H * zoom,
      opacity,
    }}
  >
    <Capture name={src} style={{width: '100%', height: '100%', display: 'block'}} />
    {children ? <div style={{position: 'absolute', inset: 0, transform: `scale(${zoom})`, transformOrigin: '0 0'}}>{children}</div> : null}
  </div>
);

/** Panneau : un rectangle de la capture (coordonnées de l'interface), affiché à `width` px. */
export const Panel: React.FC<{
  src: string;
  crop?: Rect;
  width: number;
  radius?: number;
  style?: React.CSSProperties;
  border?: boolean;
}> = ({src, crop = {x: 0, y: 0, w: UI_W, h: UI_H}, width, radius = 4, style, border = true}) => {
  const s = width / crop.w;
  return (
    <div
      style={{
        position: 'relative',
        width,
        height: crop.h * s,
        overflow: 'hidden',
        borderRadius: radius,
        boxShadow: border ? '0 0 0 1px rgba(255,255,255,0.12)' : undefined,
        background: '#0f1115',
        ...style,
      }}
    >
      <Capture
        name={src}
        style={{position: 'absolute', left: -crop.x * s, top: -crop.y * s, width: UI_W * s, height: UI_H * s, maxWidth: 'none'}}
      />
    </div>
  );
};

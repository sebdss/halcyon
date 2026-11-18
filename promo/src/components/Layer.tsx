import React from 'react';
import {Capture} from '../lib/lang';
import {UI_H, UI_W} from './Shot';

/**
 * Découpe d'une capture placée dans le repère de l'interface (1440 × 900). Superposée à la
 * capture entière au même endroit, elle s'y fond exactement ; décalée en profondeur (z), elle
 * se détache : c'est ainsi que les panneaux de l'interface se séparent pendant les mouvements.
 */
export const Layer: React.FC<{src: string; x: number; y: number; w: number; h: number; z?: number; dx?: number; dy?: number; opacity?: number}> = ({
  src,
  x,
  y,
  w,
  h,
  z = 0,
  dx = 0,
  dy = 0,
  opacity = 1,
}) => (
  <div
    style={{
      position: 'absolute',
      left: x,
      top: y,
      width: w,
      height: h,
      overflow: 'hidden',
      transform: `translate3d(${dx}px, ${dy}px, ${z}px)`,
      opacity,
      boxShadow: z > 1 || dx || dy ? `0 ${z * 0.15}px ${z * 0.5}px rgba(0,0,0,${Math.min(0.5, z / 200)})` : undefined,
    }}
  >
    <Capture name={src} style={{position: 'absolute', left: -x, top: -y, width: UI_W, height: UI_H, maxWidth: 'none'}} />
  </div>
);

/** La capture entière dans le même repère (fond des calques). */
export const Base: React.FC<{src: string; opacity?: number}> = ({src, opacity = 1}) => (
  <Capture name={src} style={{position: 'absolute', left: 0, top: 0, width: UI_W, height: UI_H, opacity}} />
);

/**
 * Plan de l'interface vu par une caméra : `focus` (repère de l'interface) placé en `at`
 * (écran), `zoom` px d'écran par px d'interface, rotations 3D et profondeur en option.
 */
export const Stage: React.FC<{
  focus: [number, number];
  zoom: number;
  at: [number, number];
  rx?: number;
  ry?: number;
  rz?: number;
  tz?: number;
  perspective?: number;
  opacity?: number;
  blur?: number;
  children: React.ReactNode;
}> = ({focus, zoom, at, rx = 0, ry = 0, rz = 0, tz = 0, perspective = 2400, opacity = 1, blur = 0, children}) => (
  <div
    style={{
      position: 'absolute',
      inset: 0,
      perspective,
      perspectiveOrigin: `${at[0]}px ${at[1]}px`,
      opacity,
      filter: blur > 0.05 ? `blur(${blur}px)` : undefined,
    }}
  >
    <div
      style={{
        position: 'absolute',
        left: 0,
        top: 0,
        width: UI_W,
        height: UI_H,
        transformOrigin: `${focus[0]}px ${focus[1]}px`,
        transform: `translate(${at[0] - focus[0]}px, ${at[1] - focus[1]}px) translateZ(${tz}px) rotateX(${rx}deg) rotateY(${ry}deg) rotateZ(${rz}deg) scale(${zoom})`,
        transformStyle: 'preserve-3d',
      }}
    >
      {children}
    </div>
  </div>
);

/** Creux : masque la zone d'un panneau dans la capture de fond quand ce panneau se soulève. */
export const Hole: React.FC<{x: number; y: number; w: number; h: number; color?: string; on?: boolean}> = ({x, y, w, h, color = '#14171c', on = true}) =>
  on ? <div style={{position: 'absolute', left: x, top: y, width: w, height: h, background: color}} /> : null;

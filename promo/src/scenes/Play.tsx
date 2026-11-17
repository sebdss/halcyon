import React from 'react';
import {AbsoluteFill, useCurrentFrame} from 'remotion';
import {Base, Stage} from '../components/Layer';
import {PlayButton} from '../components/PlayButton';
import {Reveal, display} from '../components/Type';
import {useLayout} from '../lib/layout';
import {C} from '../lib/theme';
import {ease, keys, tw} from '../lib/time';

// bouton Jouer de l'accueil, repère de l'interface
const BTN = {x: 252, y: 400, w: 156, h: 48};

/**
 * 44–48 s · L'élan. Tout s'éteint sauf Jouer ; la caméra s'en approche pendant la montée,
 * le focus s'y pose (46 s), le bouton est pressé (47 s) et son orange envahit l'image.
 * Un quart de temps de noir avant l'impact.
 */
export const Play: React.FC = () => {
  const f = useCurrentFrame();
  const {W, H, u} = useLayout();
  const cx = BTN.x + BTN.w / 2;
  const cy = BTN.y + BTN.h / 2;
  const zoom = keys(f, [0, 186], [(W / 1440) * 1.02, 3.3 * u], ease.inOut);
  const fx = keys(f, [0, 186], [720, cx]);
  const fy = keys(f, [0, 186], [450, cy]);
  const at: [number, number] = [W / 2, H / 2 + 70 * u];
  // rectangle du bouton à l'écran
  const sx = at[0] + (BTN.x - fx) * zoom;
  const sy = at[1] + (BTN.y - fy) * zoom;
  const sw = BTN.w * zoom;
  const sh = BTN.h * zoom;
  const dark = keys(f, [6, 80], [0, 0.95]);
  const ring = tw(f, [120, 132], [0, 1], ease.out);
  const pressed = f >= 180 && f < 188 ? 1 : 0;
  const take = tw(f, [186, 222], [0, 1], ease.in);
  const black = f >= 225;
  // l'orange envahit l'image à partir du bouton
  const ox = sx * (1 - take);
  const oy = sy * (1 - take);
  const ow = sw + (W - sw) * take;
  const oh = sh + (H - sh) * take;
  return (
    <AbsoluteFill style={{background: C.console, overflow: 'hidden'}}>
      <Stage focus={[fx, fy]} zoom={zoom} at={at}>
        <Base src="accueil" />
      </Stage>
      {/* tout s'éteint autour de Jouer */}
      <div
        style={{
          position: 'absolute',
          left: sx,
          top: sy,
          width: sw,
          height: sh,
          borderRadius: 5 * zoom,
          boxShadow: `0 0 0 ${4000}px rgba(14,16,20,${dark})`,
          background: pressed ? 'rgba(0,0,0,0.12)' : 'transparent',
        }}
      />
      {/* anneau de focus */}
      <div
        style={{
          position: 'absolute',
          left: sx - 3 * zoom,
          top: sy - 3 * zoom,
          width: sw + 6 * zoom,
          height: sh + 6 * zoom,
          borderRadius: 8 * zoom,
          border: `${3 * u}px solid ${C.accent}`,
          opacity: ring * (1 - take),
        }}
      />
      <div style={{position: 'absolute', left: 0, right: 0, top: sy - 270 * u, display: 'flex', flexDirection: 'column', alignItems: 'center', opacity: 1 - take}}>
        <Reveal text="Un seul geste" f={f} at={30} stagger={4} dur={22} style={display(72 * u, C.text, 800)} />
        <Reveal text="pour jouer." f={f} at={90} stagger={4} dur={22} style={display(72 * u, C.muted, 800)} />
      </div>
      {take > 0 ? (
        <div style={{position: 'absolute', left: ox, top: oy, width: ow, height: oh, borderRadius: 5 * zoom * (1 - take), background: C.warm, overflow: 'hidden'}}>
          <div style={{position: 'absolute', inset: 0, display: 'flex', alignItems: 'center', justifyContent: 'center', opacity: 1 - take * 2.5}}>
            <PlayButton height={sh} bare />
          </div>
        </div>
      ) : null}
      {black ? <AbsoluteFill style={{background: '#000'}} /> : null}
    </AbsoluteFill>
  );
};

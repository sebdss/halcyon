import React from 'react';
import {AbsoluteFill, useCurrentFrame} from 'remotion';
import {Reveal, body, display} from '../components/Type';
import {useLayout} from '../lib/layout';
import {C} from '../lib/theme';
import {ease, tw} from '../lib/time';
import {useT} from '../lib/lang';

const EMUS: [string, string][] = [
  ['RetroArch', ''], ['Dolphin', 'GameCube, Wii'], ['PCSX2', 'PlayStation 2'], ['RPCS3', 'PlayStation 3'],
  ['DuckStation', 'PlayStation'], ['PPSSPP', 'PSP'], ['Cemu', 'Wii U'], ['Ryujinx', 'Switch'], ['melonDS', 'Nintendo DS'], ['mGBA', 'Game Boy Advance'],
];
const STEP = 7.5; // une double-croche

/** 38–40 s · Les émulateurs reconnus, un par double-croche, puis la phrase. */
export const Emulation: React.FC = () => {
  const f = useCurrentFrame();
  const {u, v} = useLayout();
  const tr = useT();
  const k = Math.min(EMUS.length - 1, Math.floor(f / STEP));
  const reel = f < EMUS.length * STEP;
  const out = tw(f, [78, 88], [0, 1], ease.in);
  return (
    <AbsoluteFill style={{background: C.console, alignItems: 'center', justifyContent: 'center'}}>
      {reel || out < 1 ? (
        <div style={{textAlign: 'center', opacity: 1 - out, transform: `scale(${1 - out * 0.06})`}}>
          <div style={{...body(26 * u, C.muted, 600), marginBottom: 14 * u}}>{tr.emuKicker}</div>
          <div style={{...display((v ? 96 : 150) * u, C.text, 800)}}>{EMUS[k][0]}</div>
          <div style={{...body(32 * u, C.dim, 500), marginTop: 14 * u}}>{EMUS[k][1] || tr.emuMulti}</div>
          <div style={{margin: `${30 * u}px auto 0`, display: 'flex', gap: 8 * u, justifyContent: 'center'}}>
            {EMUS.map((_, i) => (
              <div key={i} style={{width: 18 * u, height: 4 * u, background: i <= k ? C.accent : C.surfaceActive}} />
            ))}
          </div>
        </div>
      ) : null}
      {f >= 84 ? (
        <div style={{position: 'absolute', textAlign: 'center'}}>
          <Reveal text={tr.emuTitle} f={f} at={84} stagger={2.5} dur={18} style={{...display((v ? 54 : 76) * u, C.text, 800), justifyContent: 'center', maxWidth: (v ? 900 : 1250) * u}} />
        </div>
      ) : null}
    </AbsoluteFill>
  );
};

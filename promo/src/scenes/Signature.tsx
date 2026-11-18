import React from 'react';
import {AbsoluteFill, useCurrentFrame} from 'remotion';
import {Logo} from '../components/Logo';
import {PlayButton} from '../components/PlayButton';
import {Reveal, body, display} from '../components/Type';
import {useLayout} from '../lib/layout';
import {C} from '../lib/theme';
import {ease, keys, sec, tw} from '../lib/time';
import {useT} from '../lib/lang';

/**
 * 48–57 s · La signature. L'impact : un trait orange traverse l'image et se resserre ; le logo
 * en naît, le nom le rejoint, puis la phrase, la disponibilité réelle et l'invitation.
 */
export const Signature: React.FC = () => {
  const f = useCurrentFrame();
  const {W, H, u, v} = useLayout();
  const tr = useT();
  const cx = W / 2;
  // trait d'impact
  const lineW = W * (1 - tw(f, [4, 40], [0, 1], ease.inOut));
  const lineH = keys(f, [0, 3, 40], [14 * u, 6 * u, 3 * u]);
  const flash = Math.exp(-f / 8);

  // logo et nom
  const LOGO = 26;
  const logoSize = (v ? 200 : 210) * u;
  const gap = 38 * u;
  const wordW = 686 * u;
  const lock = tw(f, [100, 144], [0, 1], ease.inOut);
  const lockW = logoSize + gap + wordW;
  const logoX = cx - logoSize / 2 - lock * (lockW / 2 - logoSize / 2);
  const rise = tw(f, [150, 190], [0, 1], ease.inOut);
  const cy = H / 2 - rise * (v ? 200 : 120) * u;
  const wordReveal = tw(f, [106, 140], [0, 100], ease.out);
  const tracking = keys(f, [106, 160], [0.38, 0], ease.out);
  const fade = tw(f, [sec(8.4), sec(9.0)], [0, 1], ease.inOut);

  const info = tr.signatureInfo;
  return (
    <AbsoluteFill style={{background: C.console, overflow: 'hidden'}}>
      <div style={{position: 'absolute', left: cx - lineW / 2, top: H / 2 - lineH / 2, width: lineW, height: lineH, background: C.warm, opacity: lineW > 1 ? 1 : 0}} />
      <div
        style={{
          position: 'absolute',
          left: cx - 700 * u,
          top: H / 2 - 300 * u,
          width: 1400 * u,
          height: 600 * u,
          background: `radial-gradient(closest-side, rgba(240,138,36,${0.32 * flash}), rgba(240,138,36,0))`,
        }}
      />
      <AbsoluteFill style={{opacity: 1 - fade}}>
        {f >= LOGO ? (
          <div style={{position: 'absolute', left: logoX, top: cy - logoSize / 2}}>
            <Logo size={logoSize} f={f - LOGO} speed={1.3} blinkAt={300} />
          </div>
        ) : null}
        <div
          style={{
            position: 'absolute',
            left: logoX + logoSize + gap,
            top: cy - 88 * u,
            clipPath: `inset(-20% ${100 - wordReveal}% -20% 0)`,
            letterSpacing: `${tracking}em`,
            whiteSpace: 'nowrap',
            ...display(136 * u, C.text, 800),
          }}
        >
          Halcyon
        </div>
        <div style={{position: 'absolute', left: 0, right: 0, top: cy + (v ? 170 : 150) * u, display: 'flex', justifyContent: 'center'}}>
          <Reveal text={tr.signatureLine} f={f} at={160} stagger={3} dur={20} style={{...display((v ? 40 : 46) * u, C.text, 700), justifyContent: 'center', maxWidth: (v ? 900 : 1700) * u}} />
        </div>
        <div style={{position: 'absolute', left: 0, right: 0, top: cy + (v ? 330 : 236) * u, display: 'flex', justifyContent: 'center', gap: 22 * u, flexWrap: 'wrap'}}>
          {info.map((t, i) => {
            const p = tw(f, [240 + i * 8, 262 + i * 8], [0, 1], ease.out);
            return (
              <div key={t} style={{display: 'flex', alignItems: 'center', gap: 22 * u, opacity: p, transform: `translateY(${(1 - p) * 12 * u}px)`}}>
                {i > 0 ? <span style={{width: 5 * u, height: 5 * u, background: C.dim}} /> : null}
                <span style={{...body(28 * u, i < 2 ? C.text : C.muted, i < 2 ? 600 : 500)}}>{t}</span>
              </div>
            );
          })}
        </div>
        <div
          style={{
            position: 'absolute',
            left: 0,
            right: 0,
            top: cy + (v ? 430 : 330) * u,
            display: 'flex',
            justifyContent: 'center',
            alignItems: 'center',
            gap: 34 * u,
            flexDirection: v ? 'column' : 'row',
            opacity: tw(f, [360, 380], [0, 1]),
            transform: `translateY(${tw(f, [360, 390], [16, 0]) * u}px)`,
          }}
        >
          <PlayButton height={64 * u} label={tr.download} icon="download" />
          <span style={{...body(30 * u, C.muted, 500)}}>github.com/sebdss/halcyon</span>
        </div>
      </AbsoluteFill>
    </AbsoluteFill>
  );
};

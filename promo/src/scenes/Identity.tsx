import React from 'react';
import {AbsoluteFill, useCurrentFrame} from 'remotion';
import {Logo} from '../components/Logo';
import {Reveal, body, display} from '../components/Type';
import {useLayout} from '../lib/layout';
import {C} from '../lib/theme';
import {ease, keys, sec, tw} from '../lib/time';
import {useT} from '../lib/lang';

/**
 * 0–10 s · L'identité. Halcyon est le martin-pêcheur : immobile au-dessus de l'eau, puis un
 * geste net. Une ligne d'eau se trace, une perche s'y pose, l'orange plonge (6 s), le logo naît
 * du point d'impact, puis le nom.
 */
export const Identity: React.FC = () => {
  const f = useCurrentFrame();
  const {W, H, u, v} = useLayout();
  const tr = useT();
  const cx = W / 2;
  const cy = H / 2 + 20 * u;
  const DIVE = sec(6.0); // impact du plongeon
  const LOGO = DIVE + 14;

  // caméra : lente poussée, légère dérive, secousse au plongeon
  const push = keys(f, [0, DIVE, 600], [1.0, 1.07, 1.0]);
  const shake = f >= DIVE && f < DIVE + 24 ? Math.sin((f - DIVE) * 2.1) * (1 - (f - DIVE) / 24) * 6 * u : 0;
  const drift = keys(f, [0, DIVE], [-14 * u, 0]);

  // ligne d'eau
  const lineW = tw(f, [10, 100], [0, (v ? 900 : 1500) * u], ease.inOut);
  const lineOut = tw(f, [LOGO + 10, LOGO + 50], [1, 0], ease.out);
  // ondulation à l'impact
  const rip = f >= DIVE ? Math.exp(-(f - DIVE) / 16) : 0;
  const ripPhase = (f - DIVE) * 0.55;
  const pts: string[] = [];
  for (let i = 0; i <= 80; i++) {
    const x = -lineW / 2 + (lineW * i) / 80;
    const d = Math.abs(x) / (260 * u);
    const y = rip * 22 * u * Math.exp(-d * d) * Math.cos(ripPhase - d * 2.4);
    pts.push(`${(cx + x).toFixed(1)},${(cy + y).toFixed(1)}`);
  }

  // lumière qui glisse sur l'eau (seul « effet » lumineux, très bas)
  const sweepX = keys(f, [0, DIVE], [cx - 700 * u, cx + 500 * u]);
  const sweepA = keys(f, [0, 60, DIVE - 30, DIVE], [0, 0.1, 0.1, 0]);

  // perche : la branche où l'oiseau se pose
  const perchIn = tw(f, [sec(4.9), sec(5.3)], [0, 1], ease.out);
  const perchOut = tw(f, [DIVE - 6, DIVE], [1, 0], ease.in);

  // plongeon : trait orange qui tombe
  const diveP = tw(f, [DIVE - 10, DIVE], [0, 1], ease.in);
  const diveOut = tw(f, [DIVE, DIVE + 14], [1, 0], ease.out);
  const flash = f >= DIVE ? Math.exp(-(f - DIVE) / 9) : 0;

  // textes
  const textOut = sec(5.35);

  // logo et nom
  const lf = f - LOGO;
  const slide = tw(f, [sec(8.0), sec(8.0) + 40], [0, 1], ease.inOut);
  const logoSize = 196 * u;
  const lockGap = 36 * u;
  const wordW = 644 * u; // largeur mesurée du mot « Halcyon » à 128 px
  const lockW = logoSize + lockGap + wordW;
  const logoX = cx - logoSize / 2 - slide * (lockW / 2 - logoSize / 2);
  const exit = tw(f, [sec(9.7), sec(10.0)], [0, 1], ease.in);
  const tracking = keys(f, [sec(8.15), sec(8.15) + 50], [0.42, 0], ease.out);
  const wordReveal = tw(f, [sec(8.15), sec(8.15) + 34], [0, 100], ease.out);

  return (
    <AbsoluteFill style={{background: C.console, overflow: 'hidden'}}>
      <AbsoluteFill
        style={{
          transform: `translate(${drift + shake}px, ${shake * 0.4}px) scale(${push * (1 - 0.04 * exit)})`,
          opacity: 1 - exit,
        }}
      >
        {/* reflet */}
        <div
          style={{
            position: 'absolute',
            left: sweepX - 500 * u,
            top: cy - 160 * u,
            width: 1000 * u,
            height: 320 * u,
            background: `radial-gradient(closest-side, rgba(61,155,233,${sweepA}), rgba(61,155,233,0))`,
          }}
        />
        <svg width={W} height={H} style={{position: 'absolute', inset: 0}}>
          <polyline points={pts.join(' ')} fill="none" stroke="rgba(236,238,241,0.22)" strokeWidth={1.4 * u} opacity={lineOut} />
          {/* perche */}
          <rect
            x={cx - 34 * u * perchIn}
            y={cy - 3 * u}
            width={68 * u * perchIn}
            height={3 * u}
            fill={C.accent}
            opacity={perchOut}
          />
          {/* plongeon */}
          <rect x={cx - 2 * u} y={cy - 520 * u * (1 - diveP) - 70 * u} width={4 * u} height={(70 + 450 * (1 - diveP)) * u * (diveP > 0 ? 1 : 0)} fill={C.warm} opacity={diveOut} />
        </svg>
        {/* éclat à l'impact */}
        <div
          style={{
            position: 'absolute',
            left: cx - 380 * u,
            top: cy - 380 * u,
            width: 760 * u,
            height: 760 * u,
            background: `radial-gradient(closest-side, rgba(240,138,36,${0.35 * flash}), rgba(240,138,36,0))`,
          }}
        />

        {/* « Immobile au-dessus de l'eau, » — « puis un geste net. » */}
        <div style={{position: 'absolute', left: 0, right: 0, top: cy - 92 * u, display: 'flex', justifyContent: 'center'}}>
          <Reveal
            text={tr.identity[0]}
            f={f}
            at={sec(2.0)}
            by="char"
            stagger={1.3}
            dur={26}
            out={textOut}
            style={{...display(40 * u, C.muted, 700)}}
          />
        </div>
        <div style={{position: 'absolute', left: 0, right: 0, top: cy + 42 * u, display: 'flex', justifyContent: 'center'}}>
          <Reveal text={tr.identity[1]} f={f} at={sec(4.0)} by="char" stagger={1.3} dur={26} out={textOut} style={{...display(40 * u, C.text, 700)}} />
        </div>

        {/* logo */}
        {lf >= 0 ? (
          <div style={{position: 'absolute', left: logoX, top: cy - logoSize / 2 - 10 * u}}>
            <Logo size={logoSize} f={lf} blinkAt={sec(9.0) - LOGO} />
          </div>
        ) : null}
        {/* nom : révélé par masque, l'approche se resserre */}
        <div
          style={{
            position: 'absolute',
            left: logoX + logoSize + lockGap,
            top: cy - 82 * u,
            clipPath: `inset(-20% ${100 - wordReveal}% -20% 0)`,
            ...display(128 * u, C.text, 800),
            letterSpacing: `${tracking}em`,
            whiteSpace: 'nowrap',
          }}
        >
          Halcyon
        </div>
        <div
          style={{
            position: 'absolute',
            left: logoX + logoSize + lockGap + 6 * u,
            top: cy + 70 * u,
            opacity: tw(f, [sec(8.6), sec(8.6) + 24], [0, 1]),
            transform: `translateY(${tw(f, [sec(8.6), sec(8.6) + 30], [10, 0]) * u}px)`,
            ...body(28 * u, C.muted, 500),
            whiteSpace: 'nowrap',
          }}
        >
          Le launcher de jeux pour Windows et Linux
        </div>
      </AbsoluteFill>
    </AbsoluteFill>
  );
};

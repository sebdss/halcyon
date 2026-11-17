import React, {useMemo} from 'react';
import {getLength} from '@remotion/paths';
import logo from '../logo.json';
import {ease, tw} from '../lib/time';

/**
 * Logo de Halcyon, vectorisé depuis crates/halcyon-app/ui/icons/logo.png et animé par parties :
 * la tuile s'ouvre, l'oiseau se dessine puis se remplit, les pattes se posent, l'œil s'ouvre.
 * `f` = image locale depuis le début de l'animation ; `speed` accélère l'ensemble.
 */
export const Logo: React.FC<{size: number; f: number; speed?: number; blinkAt?: number; style?: React.CSSProperties}> = ({
  size,
  f,
  speed = 1,
  blinkAt,
  style,
}) => {
  const len = useMemo(() => getLength(logo.bird), []);
  const k = f * speed;
  const tile = tw(k, [0, 26], [0, 1], ease.out);
  const draw = tw(k, [6, 40], [0, 1], ease.inOut);
  const fill = tw(k, [30, 46], [0, 1], ease.out);
  const feet = tw(k, [40, 52], [0, 1], ease.out);
  let eye = tw(k, [46, 54], [0, 1], ease.out);
  if (blinkAt !== undefined) {
    const b = f - blinkAt;
    if (b >= 0 && b < 10) eye *= 1 - 0.9 * Math.sin((b / 10) * Math.PI);
  }
  const [ex, ey, er] = logo.eye;
  return (
    <svg width={size} height={size} viewBox="0 0 256 256" style={{overflow: 'visible', ...style}}>
      <defs>
        <linearGradient id="hl-g" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor={logo.gradient[0]} />
          <stop offset="1" stopColor={logo.gradient[1]} />
        </linearGradient>
        <clipPath id="hl-clip">
          <circle cx="128" cy="128" r={tile * 190} />
        </clipPath>
      </defs>
      <g clipPath="url(#hl-clip)" style={{transform: `scale(${0.82 + 0.18 * tile})`, transformOrigin: '128px 128px'}}>
        <rect x="0" y="0" width="256" height="256" rx={logo.radius} fill="url(#hl-g)" />
        <path
          d={logo.bird}
          fill="#ffffff"
          fillOpacity={fill}
          stroke="#ffffff"
          strokeWidth={2.2}
          strokeLinejoin="round"
          strokeDasharray={len}
          strokeDashoffset={len * (1 - draw)}
          strokeOpacity={1 - fill * 0.999}
        />
        <g style={{transform: `scale(${feet}, ${feet})`, transformOrigin: '150px 192px'}}>
          <path d={logo.feet} fill="#f08a24" opacity={feet > 0.01 ? 1 : 0} />
        </g>
        <circle cx={ex} cy={ey} r={er} fill="#1f1a16" style={{transform: `scaleY(${eye})`, transformOrigin: `${ex}px ${ey}px`}} />
      </g>
    </svg>
  );
};

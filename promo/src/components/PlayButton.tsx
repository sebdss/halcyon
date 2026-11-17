import React from 'react';
import {C, F} from '../lib/theme';

/**
 * Le bouton Jouer / Télécharger dessiné comme `PlayButton` du launcher : orange, rayon 5 px,
 * icône pleine, libellé en Archivo Expanded 700, texte brun foncé.
 */
export const PlayButton: React.FC<{height: number; label?: string; icon?: 'play' | 'download'; bare?: boolean}> = ({
  height,
  label = 'Jouer',
  icon = 'play',
  bare = false,
}) => {
  const k = height / 48;
  return (
    <div
      style={{
        height,
        display: 'inline-flex',
        alignItems: 'center',
        gap: 12 * k,
        padding: `0 ${26 * k}px 0 ${22 * k}px`,
        borderRadius: 5 * k,
        background: bare ? 'transparent' : C.warm,
        color: C.onWarm,
        fontFamily: F.display,
        fontWeight: 700,
        fontSize: 18 * k,
        whiteSpace: 'nowrap',
      }}
    >
      {icon === 'play' ? (
        <svg width={18 * k} height={18 * k} viewBox="0 0 24 24">
          <path d="M7 4.2v15.6L20.5 12z" fill={C.onWarm} />
        </svg>
      ) : (
        <svg width={20 * k} height={20 * k} viewBox="0 0 24 24" fill="none" stroke={C.onWarm} strokeWidth={2.2} strokeLinecap="square">
          <path d="M12 4v11M7 10.5l5 5 5-5M5 19.5h14" />
        </svg>
      )}
      {label}
    </div>
  );
};

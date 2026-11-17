import React from 'react';
import {Composition} from 'remotion';
import {loadFonts} from './components/Fonts';
import {Promo, PromoProps} from './Promo';
import timeline from './timeline.json';

loadFonts();
const frames = Math.round(timeline.durationSeconds * timeline.fps);

export const Root: React.FC = () => (
  <>
    {/* Film principal : 16:9, 1920 × 1080, 60 i/s */}
    <Composition
      id="Promo"
      component={Promo}
      durationInFrames={frames}
      fps={timeline.fps}
      width={1920}
      height={1080}
      defaultProps={{audio: 'mix'} satisfies PromoProps}
    />
    {/* Déclinaison verticale 9:16 : même montage, mises en page adaptées par useLayout() */}
    <Composition
      id="PromoVertical"
      component={Promo}
      durationInFrames={frames}
      fps={timeline.fps}
      width={1080}
      height={1920}
      defaultProps={{audio: 'mix'} satisfies PromoProps}
    />
  </>
);

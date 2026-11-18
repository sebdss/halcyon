import React from 'react';
import {AbsoluteFill, Audio, Sequence, staticFile} from 'remotion';
import timeline from './timeline.json';
import {C} from './lib/theme';
import {scene, SceneId} from './lib/time';
import {Identity} from './scenes/Identity';
import {Discover} from './scenes/Discover';
import {Sources} from './scenes/Sources';
import {Views} from './scenes/Views';
import {Detail} from './scenes/Detail';
import {Console} from './scenes/Console';
import {Tools} from './scenes/Tools';
import {Emulation} from './scenes/Emulation';
import {Immersion} from './scenes/Immersion';
import {Play} from './scenes/Play';
import {Signature} from './scenes/Signature';
import {Lang, LangProvider} from './lib/lang';

const SCENES: Partial<Record<SceneId, React.FC>> = {
  identity: Identity,
  discover: Discover,
  sources: Sources,
  views: Views,
  detail: Detail,
  console: Console,
  tools: Tools,
  emulation: Emulation,
  immersion: Immersion,
  play: Play,
  signature: Signature,
};

export type PromoProps = {audio: 'mix' | 'sans-musique' | 'aucun'; lang?: Lang};

export const Promo: React.FC<PromoProps> = ({audio, lang = 'fr'}) => (
  <LangProvider lang={lang}>
  <AbsoluteFill style={{background: C.console}}>
    {timeline.scenes.map((s) => {
      const Comp = SCENES[s.id as SceneId];
      const {from, duration} = scene(s.id as SceneId);
      return Comp ? (
        <Sequence key={s.id} from={from} durationInFrames={duration} name={`${s.act} · ${s.id}`}>
          <Comp />
        </Sequence>
      ) : null;
    })}
    {audio !== 'aucun' ? (
      <Audio src={staticFile(audio === 'mix' ? 'audio/halcyon-promo-audio.wav' : 'audio/halcyon-promo-audio-sans-musique.wav')} />
    ) : null}
  </AbsoluteFill>
  </LangProvider>
);

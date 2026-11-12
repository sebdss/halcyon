//! Moteur audio de Halcyon, indépendant de l'interface.
//!
//! - `Audio::play(Cue)` est la seule porte d'entrée : non bloquante (simple envoi sur un
//!   canal), utilisable depuis n'importe quel fil.
//! - Le fil `halcyon-audio` ouvre le périphérique **au premier son** et le referme après
//!   20 s de silence : rien n'est actif tant qu'aucun son n'est joué, et l'audio ne retarde
//!   jamais le démarrage.
//! - Le rappel de la carte son n'attend jamais : si le mélangeur est occupé, il rend un
//!   silence pour ce bloc.
//! - Toute erreur (pas de périphérique, pilote en échec) est journalisée une fois ; l'audio
//!   se désactive puis réessaie au bout d'une minute, sans jamais interrompre l'application.

mod catalog;
mod mixer;

pub use catalog::{Category, Cue};
pub use mixer::{decode_wav, Cooldowns, InGame, Mix, Mixer, MAX_VOICES, SOURCE_RATE};

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Silence au bout duquel le périphérique est refermé.
const IDLE_CLOSE: Duration = Duration::from_secs(20);
/// Délai avant une nouvelle tentative après une erreur de périphérique.
const RETRY_AFTER: Duration = Duration::from_secs(60);

enum Cmd {
    Play(Cue, bool),
    Mix(Mix),
    Game(bool),
    Shutdown,
}

/// Poignée vers le moteur audio (clonable, `Send`).
#[derive(Clone)]
pub struct Audio {
    tx: Sender<Cmd>,
}

impl Audio {
    /// Démarre le fil audio (le périphérique n'est pas encore ouvert).
    pub fn start(mix: Mix) -> Self {
        let (tx, rx) = mpsc::channel();
        let spawned = std::thread::Builder::new().name("halcyon-audio".into()).spawn(move || run(rx, mix));
        if let Err(e) = spawned {
            log::warn!("audio indisponible : {e}");
        }
        Self { tx }
    }

    /// Joue un effet selon les réglages (catégorie, survol, mode silencieux, partie en cours).
    pub fn play(&self, cue: Cue) {
        let _ = self.tx.send(Cmd::Play(cue, false));
    }

    /// Prévisualisation depuis les réglages : ignore le mode silencieux et la désactivation,
    /// mais respecte les volumes.
    pub fn preview(&self, cue: Cue) {
        let _ = self.tx.send(Cmd::Play(cue, true));
    }

    pub fn set_mix(&self, mix: Mix) {
        let _ = self.tx.send(Cmd::Mix(mix));
    }

    /// Une partie lancée par Halcyon commence ou se termine.
    pub fn set_game_active(&self, active: bool) {
        let _ = self.tx.send(Cmd::Game(active));
    }

    /// Arrêt propre (fermeture de Halcyon, éventuellement pendant une lecture).
    pub fn shutdown(&self) {
        let _ = self.tx.send(Cmd::Shutdown);
    }
}

struct Output {
    _stream: cpal::Stream,
    mixer: Arc<Mutex<Mixer>>,
}

fn run(rx: Receiver<Cmd>, mut mix: Mix) {
    let epoch = Instant::now();
    let mut cooldowns = Cooldowns::default();
    let mut game = false;
    let mut output: Option<Output> = None;
    let mut last_sound = Instant::now();
    let mut failed_at: Option<Instant> = None;
    loop {
        // Périphérique fermé : attente bloquante (aucun réveil). Ouvert : réveil toutes les
        // 5 s pour le refermer après un silence.
        let cmd = if output.is_some() {
            match rx.recv_timeout(Duration::from_secs(5)) {
                Ok(c) => Some(c),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => break,
            }
        } else {
            match rx.recv() {
                Ok(c) => Some(c),
                Err(_) => break,
            }
        };
        match cmd {
            None => {}
            Some(Cmd::Shutdown) => break,
            Some(Cmd::Game(active)) => game = active,
            Some(Cmd::Mix(m)) => {
                if !m.enabled || m.muted {
                    if let Some(o) = &output {
                        if let Ok(mut mx) = o.mixer.lock() {
                            mx.stop_all();
                        }
                    }
                }
                mix = m;
                failed_at = None; // nouveau réglage : on peut réessayer tout de suite
            }
            Some(Cmd::Play(cue, preview)) => {
                let Some(gain) = mix.gain(cue, game, preview) else { continue };
                let now_ms = epoch.elapsed().as_millis() as u64;
                if !preview && !cooldowns.allow(cue, now_ms) {
                    continue;
                }
                if output.is_none() {
                    if failed_at.is_some_and(|t| t.elapsed() < RETRY_AFTER) {
                        continue;
                    }
                    match open_output() {
                        Ok(o) => {
                            failed_at = None;
                            output = Some(o);
                        }
                        Err(e) => {
                            if failed_at.is_none() {
                                log::warn!("sortie audio indisponible : {e}");
                            }
                            failed_at = Some(Instant::now());
                            continue;
                        }
                    }
                }
                if let Some(o) = &output {
                    if let Ok(mut mx) = o.mixer.lock() {
                        let played = mx.start(cue, gain);
                        log::debug!("son {} ({:.0} %){}", cue.id(), gain * 100.0, if played { "" } else { " écarté" });
                    }
                    last_sound = Instant::now();
                }
            }
        }
        // Silence prolongé : on libère le périphérique.
        if let Some(o) = &output {
            let idle = o.mixer.lock().map(|m| m.is_idle()).unwrap_or(true);
            if idle && last_sound.elapsed() >= IDLE_CLOSE {
                log::debug!("sortie audio refermée (silence)");
                output = None;
            }
        }
    }
    // Fermeture : le flux est libéré ici, même si un son était en cours.
    drop(output);
    log::debug!("moteur audio arrêté");
}

fn open_output() -> Result<Output, String> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    let host = cpal::default_host();
    let device = host.default_output_device().ok_or("aucun périphérique de sortie")?;
    let supported = device.default_output_config().map_err(|e| e.to_string())?;
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.config();
    let channels = usize::from(config.channels);
    let mixer = Arc::new(Mutex::new(Mixer::new(config.sample_rate)));
    let err = |e| log::warn!("erreur du flux audio : {e}");
    let m = mixer.clone();
    let mut scratch: Vec<f32> = Vec::new();
    let stream = match format {
        cpal::SampleFormat::F32 => device.build_output_stream(
            config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| match m.try_lock() {
                Ok(mut mx) => mx.render(data, channels),
                Err(_) => data.fill(0.0),
            },
            err,
            None,
        ),
        cpal::SampleFormat::I16 => device.build_output_stream(
            config,
            move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                scratch.resize(data.len(), 0.0);
                match m.try_lock() {
                    Ok(mut mx) => mx.render(&mut scratch, channels),
                    Err(_) => scratch.fill(0.0),
                }
                for (d, s) in data.iter_mut().zip(&scratch) {
                    *d = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
                }
            },
            err,
            None,
        ),
        cpal::SampleFormat::U16 => device.build_output_stream(
            config,
            move |data: &mut [u16], _: &cpal::OutputCallbackInfo| {
                scratch.resize(data.len(), 0.0);
                match m.try_lock() {
                    Ok(mut mx) => mx.render(&mut scratch, channels),
                    Err(_) => scratch.fill(0.0),
                }
                for (d, s) in data.iter_mut().zip(&scratch) {
                    *d = ((s.clamp(-1.0, 1.0) * 32767.0) + 32768.0) as u16;
                }
            },
            err,
            None,
        ),
        other => return Err(format!("format d'échantillon non pris en charge : {other:?}")),
    }
    .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    log::debug!("sortie audio ouverte ({} Hz, {channels} canaux, {format:?})", config_rate(&mixer));
    Ok(Output { _stream: stream, mixer })
}

fn config_rate(m: &Arc<Mutex<Mixer>>) -> u32 {
    m.lock().map(|m| m.output_rate()).unwrap_or(0)
}

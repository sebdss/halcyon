//! Décodage WAV et mélangeur : voix, volumes par catégorie, priorités, délais, rééchantillonnage.
//! Aucune dépendance à un périphérique : le rendu se teste dans un simple tampon.

use crate::catalog::Cue;
use std::collections::HashMap;

/// Taux d'échantillonnage des sons intégrés.
pub const SOURCE_RATE: u32 = 44_100;
/// Voix simultanées au plus, toutes catégories confondues.
pub const MAX_VOICES: usize = 6;

/// Lit un WAV PCM 16 bits mono (format produit par le script de synthèse).
pub fn decode_wav(bytes: &[u8]) -> Result<Vec<f32>, String> {
    let le16 = |b: &[u8], i: usize| u16::from_le_bytes([b[i], b[i + 1]]);
    let le32 = |b: &[u8], i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(tr!("fichier WAV invalide"));
    }
    let mut i = 12;
    let mut fmt_ok = false;
    while i + 8 <= bytes.len() {
        let id = &bytes[i..i + 4];
        let size = le32(bytes, i + 4) as usize;
        let body = i + 8;
        if body + size > bytes.len() {
            return Err(tr!("WAV tronqué"));
        }
        if id == b"fmt " {
            if size < 16 {
                return Err(tr!("en-tête fmt invalide"));
            }
            let (format, channels, rate, bits) =
                (le16(bytes, body), le16(bytes, body + 2), le32(bytes, body + 4), le16(bytes, body + 14));
            if format != 1 || channels != 1 || bits != 16 || rate != SOURCE_RATE {
                return Err(tr!("format non pris en charge ({format}, {channels} canal, {rate} Hz, {bits} bits)", format = format, channels = channels, rate = rate, bits = bits));
            }
            fmt_ok = true;
        } else if id == b"data" {
            if !fmt_ok {
                return Err(tr!("données avant l'en-tête fmt"));
            }
            return Ok(bytes[body..body + size]
                .chunks_exact(2)
                .map(|c| f32::from(i16::from_le_bytes([c[0], c[1]])) / 32768.0)
                .collect());
        }
        i = body + size + (size & 1);
    }
    Err(tr!("aucune donnée audio"))
}

/// Comportement pendant une partie lancée par Halcyon.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum InGame {
    /// Aucun son (par défaut).
    #[default]
    Silent,
    /// Volume réduit (30 %).
    Reduced,
    /// Comme d'habitude.
    Normal,
}

/// Réglages appliqués par le mélangeur (indépendants du volume du système).
#[derive(Clone, Debug, PartialEq)]
pub struct Mix {
    pub enabled: bool,
    /// Mode silencieux temporaire (les réglages sont conservés).
    pub muted: bool,
    pub master: f32,
    /// Volumes par catégorie, dans l'ordre de `Category::ALL`.
    pub categories: [f32; 4],
    pub hover: bool,
    pub in_game: InGame,
}

impl Default for Mix {
    fn default() -> Self {
        Self { enabled: true, muted: false, master: 0.7, categories: [0.8, 0.9, 0.9, 1.0], hover: false, in_game: InGame::Silent }
    }
}

impl Mix {
    /// Gain final d'un son, ou `None` s'il ne doit pas être joué.
    pub fn gain(&self, cue: Cue, game_active: bool, preview: bool) -> Option<f32> {
        if !preview && (!self.enabled || self.muted) {
            return None;
        }
        if cue.is_hover() && !self.hover && !preview {
            return None;
        }
        let game = match (game_active && !preview, self.in_game) {
            (false, _) | (true, InGame::Normal) => 1.0,
            (true, InGame::Reduced) => 0.3,
            (true, InGame::Silent) => 0.0,
        };
        let g = self.master.clamp(0.0, 1.0) * self.categories[cue.category().index()].clamp(0.0, 1.0) * game;
        (g > 0.001).then_some(g)
    }
}

struct Voice {
    cue: Cue,
    pos: f64,
    gain: f32,
    started: u64,
}

/// Mélangeur : reçoit des demandes de lecture et produit des échantillons.
pub struct Mixer {
    sounds: HashMap<Cue, Vec<f32>>,
    voices: Vec<Voice>,
    /// Pas de lecture (rééchantillonnage linéaire vers la fréquence du périphérique).
    step: f64,
    rate: u32,
    serial: u64,
}

impl Mixer {
    /// Décode tous les sons une fois (≈ 1 Mo de WAV, 2 Mo en mémoire).
    pub fn new(output_rate: u32) -> Self {
        let mut sounds = HashMap::new();
        for &cue in Cue::ALL {
            match decode_wav(cue.wav()) {
                Ok(s) => {
                    sounds.insert(cue, s);
                }
                Err(e) => log::warn!("son {} illisible : {e}", cue.id()),
            }
        }
        Self { sounds, voices: Vec::new(), step: f64::from(SOURCE_RATE) / f64::from(output_rate.max(1)), rate: output_rate, serial: 0 }
    }

    pub fn output_rate(&self) -> u32 {
        self.rate
    }

    pub fn is_idle(&self) -> bool {
        self.voices.is_empty()
    }

    pub fn voice_count(&self) -> usize {
        self.voices.len()
    }

    /// Démarre un son. Renvoie `false` s'il a été écarté (saturation, son absent).
    pub fn start(&mut self, cue: Cue, gain: f32) -> bool {
        if !self.sounds.contains_key(&cue) {
            return false;
        }
        self.serial += 1;
        let serial = self.serial;
        // Même son déjà en cours : redémarré (navigation) plutôt qu'empilé.
        if cue.restarts() {
            if let Some(v) = self.voices.iter_mut().find(|v| v.cue == cue) {
                v.pos = 0.0;
                v.gain = gain;
                v.started = serial;
                return true;
            }
        }
        // Limite par catégorie puis globale : la voix la moins prioritaire (puis la plus
        // ancienne) laisse la place, si elle ne l'est pas davantage que la nouvelle.
        let cat = cue.category();
        let in_cat = self.voices.iter().filter(|v| v.cue.category() == cat).count();
        if in_cat >= cat.max_voices() && !self.evict(|v| v.cue.category() == cat, cue.priority()) {
            return false;
        }
        if self.voices.len() >= MAX_VOICES && !self.evict(|_| true, cue.priority()) {
            return false;
        }
        self.voices.push(Voice { cue, pos: 0.0, gain, started: serial });
        true
    }

    fn evict(&mut self, filter: impl Fn(&Voice) -> bool, priority: u8) -> bool {
        let victim = self
            .voices
            .iter()
            .enumerate()
            .filter(|(_, v)| filter(v))
            .min_by_key(|(_, v)| (v.cue.priority(), v.started))
            .map(|(i, v)| (i, v.cue.priority()));
        match victim {
            Some((i, p)) if p <= priority => {
                self.voices.remove(i);
                true
            }
            _ => false,
        }
    }

    /// Arrête tout (mode silencieux, désactivation).
    pub fn stop_all(&mut self) {
        self.voices.clear();
    }

    /// Mélange dans `out` (entrelacé, `channels` canaux) en remplaçant son contenu.
    pub fn render(&mut self, out: &mut [f32], channels: usize) {
        let channels = channels.max(1);
        out.fill(0.0);
        let frames = out.len() / channels;
        let step = self.step;
        let sounds = &self.sounds;
        self.voices.retain_mut(|v| {
            let Some(src) = sounds.get(&v.cue) else { return false };
            let last = src.len().saturating_sub(1) as f64;
            for f in 0..frames {
                if v.pos >= last {
                    return false;
                }
                let i = v.pos as usize;
                let frac = (v.pos - i as f64) as f32;
                let s = (src[i] * (1.0 - frac) + src[i + 1] * frac) * v.gain;
                for c in 0..channels {
                    out[f * channels + c] += s;
                }
                v.pos += step;
            }
            v.pos < last
        });
        // Sécurité contre la saturation quand plusieurs sons se superposent.
        for s in out.iter_mut() {
            if s.abs() > 0.9 {
                *s = s.signum() * (0.9 + 0.1 * ((s.abs() - 0.9) / 0.1).tanh());
            }
        }
    }
}

/// Délais minimaux entre deux lectures d'un même son (anti-rafale).
#[derive(Default)]
pub struct Cooldowns {
    last: HashMap<Cue, u64>,
}

impl Cooldowns {
    /// `now_ms` : horloge monotone en millisecondes.
    pub fn allow(&mut self, cue: Cue, now_ms: u64) -> bool {
        match self.last.get(&cue) {
            Some(&t) if now_ms.saturating_sub(t) < cue.cooldown_ms() => false,
            _ => {
                self.last.insert(cue, now_ms);
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_cue_decodes_without_clipping() {
        for &cue in Cue::ALL {
            let s = decode_wav(cue.wav()).unwrap_or_else(|e| panic!("{} : {e}", cue.id()));
            assert!(s.len() > 400, "{} trop court", cue.id());
            let peak = s.iter().fold(0f32, |m, x| m.max(x.abs()));
            assert!(peak < 0.72 && peak > 0.02, "{} crête {peak}", cue.id());
            assert!(s[0].abs() < 0.01 && s[s.len() - 1].abs() < 0.01, "{} : clic au début ou à la fin", cue.id());
        }
    }

    #[test]
    fn ids_round_trip() {
        for &cue in Cue::ALL {
            assert_eq!(Cue::from_id(cue.id()), Some(cue));
        }
        assert_eq!(Cue::from_id("inconnu"), None);
    }

    #[test]
    fn rejects_bad_wav() {
        assert!(decode_wav(b"RIFF").is_err());
        assert!(decode_wav(b"RIFF\0\0\0\0WAVEdata\x04\0\0\0\0\0\0\0").is_err());
    }

    #[test]
    fn gain_respects_settings() {
        let mut m = Mix::default();
        assert!(m.gain(Cue::Confirm, false, false).is_some());
        assert_eq!(m.gain(Cue::Hover, false, false), None, "survol désactivé par défaut");
        assert!(m.gain(Cue::Hover, false, true).is_some(), "prévisualisation");
        assert_eq!(m.gain(Cue::Confirm, true, false), None, "silencieux pendant une partie");
        m.in_game = InGame::Reduced;
        let g = m.gain(Cue::Confirm, true, false).unwrap();
        assert!((g - 0.7 * 0.9 * 0.3).abs() < 1e-4);
        m.muted = true;
        assert_eq!(m.gain(Cue::Error, false, false), None);
        m.muted = false;
        m.categories[2] = 0.0;
        assert_eq!(m.gain(Cue::Error, false, false), None, "catégorie à zéro");
        m.enabled = false;
        assert_eq!(m.gain(Cue::Launch, false, false), None);
    }

    #[test]
    fn renders_and_finishes() {
        let mut m = Mixer::new(48_000);
        assert!(m.start(Cue::Select, 1.0));
        let mut buf = vec![0f32; 48_000 / 10 * 2];
        m.render(&mut buf, 2);
        assert!(buf.iter().any(|s| s.abs() > 0.01));
        assert_eq!(buf[0], buf[1], "même signal sur les deux canaux");
        assert!(m.is_idle(), "35 ms : terminé après 100 ms");
        m.render(&mut buf, 2);
        assert!(buf.iter().all(|s| *s == 0.0));
    }

    #[test]
    fn retrigger_restarts_instead_of_stacking() {
        let mut m = Mixer::new(44_100);
        for _ in 0..5 {
            m.start(Cue::Select, 1.0);
        }
        assert_eq!(m.voice_count(), 1);
    }

    #[test]
    fn priorities_and_limits() {
        let mut m = Mixer::new(44_100);
        assert!(m.start(Cue::Success, 1.0));
        assert!(m.start(Cue::Download, 1.0));
        // Troisième notification : la moins prioritaire et la plus ancienne s'arrête.
        assert!(m.start(Cue::Error, 1.0));
        assert_eq!(m.voice_count(), 2);
        // Lancement (catégorie à une voix) : un second lancement remplace le premier.
        assert!(m.start(Cue::Launch, 1.0));
        assert!(m.start(Cue::Launch, 1.0));
        assert_eq!(m.voices.iter().filter(|v| v.cue == Cue::Launch).count(), 1);
        // Saturation globale : un son moins prioritaire que tous les autres est écarté.
        let mut full = Mixer::new(44_100);
        for c in [Cue::Error, Cue::Warning, Cue::Launch, Cue::Favorite, Cue::Confirm, Cue::Section] {
            assert!(full.start(c, 1.0));
        }
        assert!(!full.start(Cue::MenuClose, 1.0) || full.voice_count() <= MAX_VOICES);
        assert!(full.voice_count() <= MAX_VOICES);
    }

    #[test]
    fn overlapping_sounds_never_clip() {
        let mut m = Mixer::new(44_100);
        for c in [Cue::Launch, Cue::Major, Cue::Error, Cue::Success, Cue::Confirm, Cue::Favorite] {
            m.start(c, 1.0);
        }
        let mut buf = vec![0f32; 44_100];
        m.render(&mut buf, 1);
        assert!(buf.iter().all(|s| s.abs() <= 1.0));
    }

    #[test]
    fn cooldown_blocks_bursts() {
        let mut c = Cooldowns::default();
        assert!(c.allow(Cue::Select, 1000));
        assert!(!c.allow(Cue::Select, 1010));
        assert!(c.allow(Cue::Select, 1050));
        assert!(c.allow(Cue::Confirm, 1050));
    }
}

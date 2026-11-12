//! Écoute de tous les sons par le vrai chemin de sortie (périphérique par défaut).
//! `cargo run -p halcyon-audio --example audition [identifiant…]`
use halcyon_audio::{Audio, Cue, Mix};
use std::time::Duration;

fn main() {
    let ids: Vec<String> = std::env::args().skip(1).collect();
    let mix = Mix { hover: true, ..Mix::default() };
    let audio = Audio::start(mix);
    for &cue in Cue::ALL {
        if !ids.is_empty() && !ids.iter().any(|i| i == cue.id()) {
            continue;
        }
        println!("{:20} {}", cue.id(), cue.description());
        audio.play(cue);
        std::thread::sleep(Duration::from_millis(1300));
    }
    audio.shutdown();
    std::thread::sleep(Duration::from_millis(200));
}

//! Identité sonore : liaison entre Halcyon et le moteur `halcyon-audio`.
//!
//! Point d'entrée unique : `Controller::sound(Cue)`. Les évènements de Halcyon (navigation,
//! sélection, lancement, notifications, tâches…) appellent cette méthode depuis leurs
//! fonctions existantes ; l'interface passe par le global Slint `Sound`.

use crate::controller::Controller;
use crate::{SettingsUi, Sound};
use halcyon_audio::{Cue, InGame, Mix};
use halcyon_storage::settings::SoundSettings;
use slint::{ComponentHandle, ModelRc, VecModel};
use std::rc::Rc;

/// Réglages enregistrés → réglages du mélangeur.
pub fn mix_from(s: &SoundSettings) -> Mix {
    Mix {
        enabled: s.enabled,
        muted: s.muted,
        master: s.master,
        categories: [s.navigation, s.interaction, s.notification, s.important],
        hover: s.hover,
        in_game: match s.in_game {
            1 => InGame::Reduced,
            2 => InGame::Normal,
            _ => InGame::Silent,
        },
    }
}

impl Controller {
    /// Joue un effet (non bloquant ; ignoré selon les réglages).
    pub(crate) fn sound(&self, cue: Cue) {
        self.audio.play(cue);
    }

    pub(crate) fn publish_sound_ui(&self) {
        let Some(app) = self.app() else { return };
        let s = self.settings.borrow().sound.clone();
        let ui = app.global::<SettingsUi>();
        ui.set_sound_enabled(s.enabled);
        ui.set_sound_muted(s.muted);
        ui.set_sound_hover(s.hover);
        ui.set_sound_in_game(i32::from(s.in_game));
        ui.set_sound_volumes(ModelRc::new(VecModel::from(vec![s.master, s.navigation, s.interaction, s.notification, s.important])));
    }

    fn edit_sound(&self, f: impl FnOnce(&mut SoundSettings)) {
        let mix = {
            let mut st = self.settings.borrow_mut();
            f(&mut st.sound);
            mix_from(&st.sound)
        };
        self.audio.set_mix(mix);
        self.save_settings();
        self.publish_sound_ui();
    }

    pub(crate) fn sound_toggle(&self, key: &str, on: bool) {
        self.edit_sound(|s| match key {
            "enabled" => s.enabled = on,
            "muted" => s.muted = on,
            "hover" => s.hover = on,
            _ => {}
        });
    }

    fn sound_volume(&self, index: i32, value: f32) {
        let v = value.clamp(0.0, 1.0);
        self.edit_sound(|s| match index {
            0 => s.master = v,
            1 => s.navigation = v,
            2 => s.interaction = v,
            3 => s.notification = v,
            4 => s.important = v,
            5 => s.in_game = (value.round() as i32).clamp(0, 2) as u8,
            _ => {}
        });
        // Le réglage s'entend : un son de la catégorie modifiée (le volume général fait
        // entendre la navigation).
        let cue = match index {
            2 => Cue::Confirm,
            3 => Cue::Success,
            4 => Cue::Launched,
            5 => return,
            _ => Cue::Section,
        };
        self.audio.preview(cue);
    }

    fn sound_preview(&self, index: i32) {
        let cue = match index {
            1 => Cue::Section,
            2 => Cue::Favorite,
            3 => Cue::Success,
            4 => Cue::Launch,
            _ => Cue::Confirm,
        };
        self.audio.preview(cue);
    }

    fn sound_reset(&self) {
        self.edit_sound(|s| *s = SoundSettings::default());
        self.toast(tr!("Réglages du son rétablis"), false);
    }

    pub(crate) fn wire_sound(self: &Rc<Self>, app: &crate::AppWindow) {
        let weak = Rc::downgrade(self);
        macro_rules! with {
            (|$c:ident $(, $arg:ident)*| $body:expr) => {{
                let weak = weak.clone();
                move |$($arg),*| {
                    if let Some($c) = weak.upgrade() {
                        $body
                    }
                }
            }};
        }
        app.global::<Sound>().on_play(with!(|c, id| {
            match Cue::from_id(&id) {
                Some(cue) => c.sound(cue),
                None => log::debug!("son inconnu demandé par l'interface : {id}"),
            }
        }));
        let ui = app.global::<SettingsUi>();
        ui.on_sound_toggle(with!(|c, key, on| c.sound_toggle(&key, on)));
        ui.on_sound_volume(with!(|c, i, v| c.sound_volume(i, v)));
        ui.on_sound_preview(with!(|c, i| c.sound_preview(i)));
        ui.on_sound_reset(with!(|c| c.sound_reset()));
        self.publish_sound_ui();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_map_to_mixer() {
        let mut s = SoundSettings { in_game: 1, hover: true, ..Default::default() };
        s.notification = 0.25;
        let m = mix_from(&s);
        assert_eq!(m.in_game, InGame::Reduced);
        assert!(m.hover);
        assert_eq!(m.categories[2], 0.25);
        assert_eq!(mix_from(&SoundSettings::default()).in_game, InGame::Silent);
    }
}

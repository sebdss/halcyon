//! Paramètres › Langue : liste des langues, choix et changement à chaud.

use crate::controller::Controller;
use crate::{Nav, SettingsUi};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

impl Controller {
    /// Liste « Automatique (…) » + langues disponibles, et choix courant.
    pub(crate) fn refresh_language_ui(&self) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<SettingsUi>();
        let langs = halcyon_i18n::languages();
        let detected = halcyon_i18n::startup_language(None);
        let detected_name = langs.iter().find(|l| l.code == detected).map(|l| l.native_name.clone()).unwrap_or(detected);
        let mut names: Vec<SharedString> = vec![tr!("Automatique (langue du système)").into()];
        names.extend(langs.iter().map(|l| SharedString::from(l.native_name.as_str())));
        let pref = self.settings.borrow().language.clone();
        let index = pref.and_then(|p| langs.iter().position(|l| l.code == p)).map_or(0, |i| i as i32 + 1);
        ui.set_languages(ModelRc::new(VecModel::from(names)));
        ui.set_language_index(index);
        let system = halcyon_i18n::system_locales();
        let available: Vec<String> = langs.iter().map(|l| l.code.clone()).collect();
        ui.set_language_fallback(!halcyon_i18n::system_language_supported(&system, &available));
        ui.set_language_system(system.first().cloned().unwrap_or_default().into());
        ui.set_language_detected(detected_name.into());
    }

    /// Choix d'une langue (0 : automatique) : enregistré, appliqué immédiatement.
    pub(crate) fn set_ui_language(&self, index: i32) {
        let langs = halcyon_i18n::languages();
        let choice = usize::try_from(index - 1).ok().and_then(|i| langs.get(i)).map(|l| l.code.clone());
        if self.settings.borrow().language == choice {
            return;
        }
        self.settings.borrow_mut().language = choice.clone();
        self.save_settings();
        let code = halcyon_i18n::startup_language(choice.as_deref());
        halcyon_i18n::set_language(&code);
        log::info!("langue de l'interface : {code}");
        crate::lang::apply_to_ui();
        self.relocalize();
    }

    /// Recalcule les textes produits côté Rust après un changement de langue ; la page,
    /// l'onglet, la recherche et la sélection en cours sont conservés.
    pub(crate) fn relocalize(&self) {
        let Some(app) = self.app() else { return };
        self.refresh_language_ui();
        self.refresh_all();
        self.refresh_home();
        self.refresh_system_ui();
        self.refresh_appearance_ui();
        self.publish_tasks();
        self.publish_sound_ui();
        let q = app.global::<SettingsUi>().get_search().to_string();
        self.settings_search(&q);
        self.on_page_shown(app.global::<Nav>().get_page());
        // Menu de l'icône de notification : reconstruit dans la nouvelle langue.
        if self.tray.borrow().is_some() {
            *self.tray.borrow_mut() = None;
            self.ensure_tray();
        }
    }
}

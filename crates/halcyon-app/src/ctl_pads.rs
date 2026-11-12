//! Page « Manettes » : manettes connectées, profils de touches (navigation de
//! Halcyon et Big Picture) et options transmises aux émulateurs.

use crate::controller::Controller;
use crate::gamepad::{self, PadEvent, PadInfo, CONFIGURABLE};
use crate::{BpUi, EmuArgRow, MappingRow, PadProfileRow, PadRow, PadsUi};
use halcyon_storage::settings::PadProfile;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::rc::Rc;
use std::time::Duration;

impl Controller {
    pub(crate) fn on_pad_event(self: &Rc<Self>, ev: PadEvent) {
        match ev {
            PadEvent::Action(a) => self.on_pad(a),
            PadEvent::Devices(list) => {
                let connected = !list.is_empty();
                self.pad_connected.set(connected);
                *self.pads.borrow_mut() = list;
                if let Some(app) = self.app() {
                    app.global::<BpUi>().set_pad_connected(connected);
                }
                self.refresh_pads_ui();
            }
            PadEvent::Captured(button) => self.on_button_captured(&button),
        }
    }

    /// Applique au thread des manettes les touches du profil actif.
    pub(crate) fn apply_pad_profile(&self) {
        let s = self.settings.borrow();
        let mapping = s.pad_profiles.iter().find(|p| p.id == s.active_pad_profile).map(|p| p.mapping.clone()).unwrap_or_default();
        gamepad::set_mapping(&mapping);
    }

    fn pad_brand(&self) -> String {
        let pads = self.pads.borrow();
        pads.iter().find(|p| p.last_used).or_else(|| pads.first()).map(|p| p.brand.clone()).unwrap_or_else(|| "Xbox".into())
    }

    fn mapping_summary(&self, p: &PadProfile, brand: &str) -> String {
        let mut parts: Vec<String> = CONFIGURABLE
            .iter()
            .filter_map(|(key, label, _, default)| {
                let b = p.mapping.get(*key)?;
                (Some(b.as_str()) != gamepad::button_name(*default)).then(|| format!("{label} : {}", gamepad::button_label(b, brand)))
            })
            .collect();
        let emu = p.emulator_args.values().filter(|v| !v.trim().is_empty()).count();
        if emu > 0 {
            parts.push(format!("options pour {emu} émulateur(s)"));
        }
        if parts.is_empty() { "Disposition standard".into() } else { parts.join(" · ") }
    }

    pub(crate) fn refresh_pads_ui(&self) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<PadsUi>();
        let brand = self.pad_brand();
        let pads: Vec<PadRow> = self
            .pads
            .borrow()
            .iter()
            .map(|p: &PadInfo| PadRow { name: p.name.clone().into(), brand: p.brand.clone().into(), power: p.power.clone().into(), last_used: p.last_used })
            .collect();
        ui.set_pads(ModelRc::new(VecModel::from(pads)));
        let s = self.settings.borrow();
        let rows: Vec<PadProfileRow> = s
            .pad_profiles
            .iter()
            .map(|p| PadProfileRow {
                id: p.id as i32,
                name: p.name.clone().into(),
                summary: self.mapping_summary(p, &brand).into(),
                active: p.id == s.active_pad_profile,
            })
            .collect();
        let mut labels: Vec<SharedString> = vec!["Disposition standard".into()];
        labels.extend(s.pad_profiles.iter().map(|p| SharedString::from(p.name.as_str())));
        let active = s.pad_profiles.iter().position(|p| p.id == s.active_pad_profile).map(|i| i + 1).unwrap_or(0);
        drop(s);
        ui.set_profiles(ModelRc::new(VecModel::from(rows)));
        ui.set_profile_labels(ModelRc::new(VecModel::from(labels)));
        ui.set_active(active as i32);
        self.refresh_pad_editor();
    }

    fn refresh_pad_editor(&self) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<PadsUi>();
        let edit = self.pad_edit.borrow();
        let Some(p) = edit.as_ref() else { return };
        let brand = self.pad_brand();
        let mapping: Vec<MappingRow> = CONFIGURABLE
            .iter()
            .map(|(key, label, _, default)| {
                let b = p.mapping.get(*key).cloned().unwrap_or_else(|| gamepad::button_name(*default).unwrap_or("").to_string());
                MappingRow { action: (*key).into(), label: (*label).into(), button: gamepad::button_label(&b, &brand).into() }
            })
            .collect();
        ui.set_mapping(ModelRc::new(VecModel::from(mapping)));
        let reg = self.registry.borrow();
        let detected = self.detected.borrow();
        let mut emus: Vec<EmuArgRow> = reg
            .emulators
            .iter()
            .filter(|d| detected.contains_key(&d.id) || p.emulator_args.contains_key(&d.id))
            .map(|d| EmuArgRow {
                id: d.id.clone().into(),
                name: d.name.clone().into(),
                args: p.emulator_args.get(&d.id).cloned().unwrap_or_default().into(),
            })
            .collect();
        emus.sort_by(|a, b| a.name.cmp(&b.name));
        ui.set_emu_args(ModelRc::new(VecModel::from(emus)));
    }

    pub(crate) fn set_active_pad_profile(&self, index: usize) {
        let id = index.checked_sub(1).and_then(|i| self.settings.borrow().pad_profiles.get(i).map(|p| p.id)).unwrap_or(0);
        self.settings.borrow_mut().active_pad_profile = id;
        self.save_settings();
        self.apply_pad_profile();
        self.refresh_pads_ui();
    }

    fn open_pad_editor(&self, p: PadProfile) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<PadsUi>();
        ui.set_edit_name(p.name.clone().into());
        ui.set_editing(p.id as i32);
        ui.set_capture("".into());
        *self.pad_edit.borrow_mut() = Some(p);
        self.refresh_pad_editor();
    }

    pub(crate) fn new_pad_profile(&self) {
        self.open_pad_editor(PadProfile::default());
    }

    pub(crate) fn edit_pad_profile(&self, id: i64) {
        let p = self.settings.borrow().pad_profiles.iter().find(|p| p.id == id).cloned();
        if let Some(p) = p {
            self.open_pad_editor(p);
        }
    }

    pub(crate) fn start_capture(self: &Rc<Self>, action: &str) {
        let Some(app) = self.app() else { return };
        if self.pads.borrow().is_empty() {
            self.toast("Aucune manette connectée", true);
            return;
        }
        app.global::<PadsUi>().set_capture(action.into());
        gamepad::set_capture(true);
        // Abandon automatique si aucun bouton n'est pressé.
        let weak = Rc::downgrade(self);
        let action = action.to_string();
        slint::Timer::single_shot(Duration::from_secs(8), move || {
            if let Some(c) = weak.upgrade() {
                if c.app().is_some_and(|a| a.global::<PadsUi>().get_capture() == action) {
                    c.cancel_capture();
                }
            }
        });
    }

    pub(crate) fn cancel_capture(&self) {
        gamepad::set_capture(false);
        if let Some(app) = self.app() {
            app.global::<PadsUi>().set_capture("".into());
        }
    }

    fn on_button_captured(&self, button: &str) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<PadsUi>();
        let action = ui.get_capture().to_string();
        ui.set_capture("".into());
        let mut edit = self.pad_edit.borrow_mut();
        let Some(p) = edit.as_mut() else { return };
        if action.is_empty() {
            return;
        }
        let current = |p: &PadProfile, key: &str| -> String {
            p.mapping.get(key).cloned().unwrap_or_else(|| {
                CONFIGURABLE.iter().find(|c| c.0 == key).and_then(|c| gamepad::button_name(c.3)).unwrap_or("").to_string()
            })
        };
        let previous = current(p, &action);
        // Un bouton ne sert qu'à une action : celle qui l'utilisait reprend l'ancien bouton.
        let other = CONFIGURABLE.iter().map(|c| c.0).find(|k| *k != action && current(p, k) == button);
        if let Some(o) = other {
            p.mapping.insert(o.to_string(), previous);
        }
        p.mapping.insert(action, button.to_string());
        drop(edit);
        self.refresh_pad_editor();
    }

    pub(crate) fn set_pad_emu_arg(&self, emulator: &str, args: &str) {
        if let Some(p) = self.pad_edit.borrow_mut().as_mut() {
            if args.trim().is_empty() {
                p.emulator_args.remove(emulator);
            } else {
                p.emulator_args.insert(emulator.to_string(), args.trim().to_string());
            }
        }
    }

    pub(crate) fn save_pad_profile(&self, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            self.toast("Donnez un nom au profil", true);
            return;
        }
        self.cancel_capture();
        let Some(mut p) = self.pad_edit.borrow_mut().take() else { return };
        p.name = name.to_string();
        {
            let mut s = self.settings.borrow_mut();
            if p.id == 0 {
                p.id = s.pad_profiles.iter().map(|x| x.id).max().unwrap_or(0) + 1;
                s.pad_profiles.push(p.clone());
            } else if let Some(x) = s.pad_profiles.iter_mut().find(|x| x.id == p.id) {
                *x = p.clone();
            }
        }
        self.save_settings();
        if let Some(app) = self.app() {
            app.global::<PadsUi>().set_editing(-1);
        }
        self.apply_pad_profile();
        self.refresh_pads_ui();
        self.toast(format!("Profil « {} » enregistré", p.name), false);
    }

    pub(crate) fn delete_pad_profile(self: &Rc<Self>, id: i64) {
        let Some(name) = self.settings.borrow().pad_profiles.iter().find(|p| p.id == id).map(|p| p.name.clone()) else { return };
        self.confirm(
            &format!("Supprimer le profil « {name} » ?"),
            "Les profils de lancement qui l'utilisaient reviendront au profil actif.",
            "Supprimer",
            move |c| {
                {
                    let mut s = c.settings.borrow_mut();
                    s.pad_profiles.retain(|p| p.id != id);
                    if s.active_pad_profile == id {
                        s.active_pad_profile = 0;
                    }
                }
                c.save_settings();
                if c.pad_edit.borrow().as_ref().is_some_and(|p| p.id == id) {
                    *c.pad_edit.borrow_mut() = None;
                    if let Some(app) = c.app() {
                        app.global::<PadsUi>().set_editing(-1);
                    }
                }
                c.apply_pad_profile();
                c.refresh_pads_ui();
            },
        );
    }
}

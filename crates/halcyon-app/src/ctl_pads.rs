//! Manettes : branchement, débranchement et touches de navigation (profil actif ou
//! disposition standard). La page de configuration a été retirée en 1.0.0 ; les profils
//! déjà enregistrés restent appliqués.

use crate::controller::Controller;
use crate::gamepad::{self, PadEvent};
use crate::BpUi;
use slint::ComponentHandle;
use std::rc::Rc;

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
            }
        }
    }

    /// Applique au thread des manettes les touches du profil actif.
    pub(crate) fn apply_pad_profile(&self) {
        let s = self.settings.borrow();
        let mapping = s.pad_profiles.iter().find(|p| p.id == s.active_pad_profile).map(|p| p.mapping.clone()).unwrap_or_default();
        gamepad::set_mapping(&mapping);
    }
}

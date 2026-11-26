//! Barre de titre intégrée (Windows, 1.1.0).
//!
//! Sous Windows, la fenêtre principale n'a plus de cadre système : la barre supérieure de
//! Halcyon porte les boutons Réduire / Agrandir / Fermer, et ses zones vides déplacent la
//! fenêtre. Le déplacement et le redimensionnement passent par le système (`drag_window`,
//! `drag_resize_window`) : ancrage aux bords (Aero Snap), Win + flèches et écrans multiples
//! restent ceux de Windows. Sous Linux, le cadre du bureau est conservé (GNOME, KDE…).

use crate::controller::Controller;
use crate::AppUi;
use slint::winit_030::winit::window::ResizeDirection;
use slint::winit_030::WinitWindowAccessor;
use slint::ComponentHandle;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// Deux appuis plus rapprochés que ce délai valent un double-clic (agrandir / restaurer).
const DOUBLE_CLICK: Duration = Duration::from_millis(450);

/// Vrai si la barre de titre intégrée remplace celle du système.
/// `HALCYON_SYSTEM_FRAME=1` rend le cadre du système ; `HALCYON_CUSTOM_FRAME=1` force la
/// barre intégrée ailleurs que sous Windows (essais).
pub(crate) fn custom_frame() -> bool {
    if std::env::var_os("HALCYON_SYSTEM_FRAME").is_some() {
        return false;
    }
    cfg!(windows) || std::env::var_os("HALCYON_CUSTOM_FRAME").is_some()
}

impl Controller {
    pub(crate) fn wire_frame(self: &Rc<Self>, app: &crate::AppWindow) {
        let ui = app.global::<AppUi>();
        ui.set_custom_frame(custom_frame());
        if !custom_frame() {
            return;
        }
        let weak = Rc::downgrade(self);
        let with = move |f: fn(&Controller)| {
            let weak = weak.clone();
            move || {
                if let Some(c) = weak.upgrade() {
                    f(&c);
                }
            }
        };
        ui.on_window_drag(with(Controller::window_drag));
        ui.on_window_minimize(with(Controller::window_minimize));
        ui.on_window_toggle_maximize(with(Controller::window_toggle_maximize));
        ui.on_window_close(with(Controller::window_close));
        ui.on_window_geometry_changed(with(Controller::refresh_frame_state));
        let weak = Rc::downgrade(self);
        ui.on_window_resize(move |edge| {
            if let Some(c) = weak.upgrade() {
                c.window_resize(edge);
            }
        });
    }

    /// À appeler une fois la fenêtre affichée : ombre portée de Windows sur la fenêtre sans
    /// cadre, et état agrandi initial.
    pub(crate) fn frame_shown(&self) {
        if !custom_frame() {
            return;
        }
        if let Some(app) = self.app() {
            #[cfg(windows)]
            app.window().with_winit_window(|w| {
                use slint::winit_030::winit::platform::windows::WindowExtWindows;
                w.set_undecorated_shadow(true);
            });
            let _ = app;
        }
        self.refresh_frame_state();
    }

    fn refresh_frame_state(&self) {
        if let Some(app) = self.app() {
            let maximized = app.window().is_maximized();
            let ui = app.global::<AppUi>();
            if ui.get_maximized() != maximized {
                ui.set_maximized(maximized);
            }
        }
    }

    fn window_drag(&self) {
        let Some(app) = self.app() else { return };
        let now = Instant::now();
        let double = self.frame_last_press.get().is_some_and(|t| now.duration_since(t) < DOUBLE_CLICK);
        if double {
            self.frame_last_press.set(None);
            self.window_toggle_maximize();
            return;
        }
        self.frame_last_press.set(Some(now));
        app.window().with_winit_window(|w| {
            if let Err(e) = w.drag_window() {
                log::debug!("déplacement de la fenêtre impossible : {e}");
            }
        });
    }

    fn window_resize(&self, edge: i32) {
        let dir = match edge {
            0 => ResizeDirection::North,
            1 => ResizeDirection::South,
            2 => ResizeDirection::East,
            3 => ResizeDirection::West,
            4 => ResizeDirection::NorthEast,
            5 => ResizeDirection::NorthWest,
            6 => ResizeDirection::SouthEast,
            _ => ResizeDirection::SouthWest,
        };
        if let Some(app) = self.app() {
            app.window().with_winit_window(|w| {
                if let Err(e) = w.drag_resize_window(dir) {
                    log::debug!("redimensionnement de la fenêtre impossible : {e}");
                }
            });
        }
    }

    fn window_minimize(&self) {
        if let Some(app) = self.app() {
            app.window().set_minimized(true);
        }
    }

    fn window_toggle_maximize(&self) {
        if let Some(app) = self.app() {
            let w = app.window();
            w.set_maximized(!w.is_maximized());
        }
        self.refresh_frame_state();
    }

    /// Même effet que la croix du système : zone de notification ou arrêt complet.
    fn window_close(&self) {
        let _ = self.on_close_requested();
        if let Some(app) = self.app() {
            let _ = app.hide();
        }
    }
}

//! Intégration au système : zone de notification, démarrage avec la session,
//! raccourcis clavier (personnalisables et global), lancement rapide, mode compact,
//! fenêtre compagnon, instance unique, raccourcis de jeux, intégrité et mise à jour.

use crate::controller::{post, Controller};
use crate::models::card_for;
use crate::tray::{Tray, TrayEvent};
use crate::{
    AppUi, CompanionWindow, DetailUi, MiniRow, MiniWindow, Nav, Page, QuickRow, QuickUi, SettingsUi, ShortcutRow, SystemUi,
};
use halcyon_core::model::GameId;
use halcyon_core::search::{MatchTier, SearchQuery};
use halcyon_storage::now_unix;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::rc::Rc;
use std::time::Duration;

/// Actions associables à un raccourci (identifiant, libellé, combinaison par défaut).
pub const SHORTCUTS: [(&str, &str, &str); 7] = [
    ("quick_launch", tr_noop!("Lancement rapide"), "Ctrl+K"),
    ("search", tr_noop!("Rechercher dans la bibliothèque"), "Ctrl+F"),
    ("big_picture", tr_noop!("Big Picture"), "F11"),
    ("mini_mode", tr_noop!("Mode compact"), "Ctrl+M"),
    ("rescan", tr_noop!("Analyser les sources"), "F5"),
    ("surprise", tr_noop!("Surprends-moi"), "Ctrl+J"),
    ("settings", tr_noop!("Paramètres"), "Ctrl+,"),
];

/// Ctrl+1 … Ctrl+9 : pages (non modifiable).
const NAV_PAGES: [Page; 9] =
    [Page::Home, Page::Library, Page::Assistant, Page::Achievements, Page::Stats, Page::Journal, Page::Updates, Page::Saves, Page::Settings];

/// Pages proposées par le lancement rapide (libellé, mots-clés, page).
const QUICK_PAGES: [(&str, &str, Page); 19] = [
    (tr_noop!("Accueil"), trc_noop!("keywords", "home debut"), Page::Home),
    (tr_noop!("Bibliothèque"), trc_noop!("keywords", "jeux library"), Page::Library),
    (tr_noop!("Que jouer ?"), trc_noop!("keywords", "assistant suggestions recommandations"), Page::Assistant),
    (tr_noop!("Succès"), trc_noop!("keywords", "achievements trophees"), Page::Achievements),
    (tr_noop!("Statistiques"), trc_noop!("keywords", "stats temps graphiques"), Page::Stats),
    (tr_noop!("Journal"), trc_noop!("keywords", "activite historique"), Page::Journal),
    (tr_noop!("Mises à jour"), trc_noop!("keywords", "updates telechargements"), Page::Updates),
    (tr_noop!("Sauvegardes"), trc_noop!("keywords", "saves backup"), Page::Saves),
    (tr_noop!("Émulateurs"), trc_noop!("keywords", "emulation roms retro"), Page::Emulators),
    (tr_noop!("Manettes"), trc_noop!("keywords", "controller gamepad pad"), Page::Controllers),
    (tr_noop!("Jeu à distance"), trc_noop!("keywords", "streaming sunshine moonlight remote"), Page::Remote),
    (tr_noop!("Performances"), trc_noop!("keywords", "performance center fps cpu gpu ram processeur memoire carte graphique monitoring"), Page::Performance),
    (tr_noop!("Minecraft"), trc_noop!("keywords", "java mods modpacks fabric forge neoforge"), Page::Minecraft),
    (tr_noop!("Téléchargements"), trc_noop!("keywords", "downloads file attente pause debit vitesse"), Page::Downloads),
    (tr_noop!("Extensions"), trc_noop!("keywords", "plugins"), Page::Plugins),
    (tr_noop!("Apparence"), trc_noop!("keywords", "theme couleurs fond"), Page::Appearance),
    (tr_noop!("Paramètres"), trc_noop!("keywords", "settings options reglages raccourcis"), Page::Settings),
    (tr_noop!("Paramètres › Raccourcis"), trc_noop!("keywords", "clavier hotkey"), Page::Settings),
    (tr_noop!("Paramètres › Exporter la configuration"), trc_noop!("keywords", "backup restauration"), Page::Settings),
];

/// Combinaison lisible (« Ctrl+Shift+K », « F5 ») à partir d'un évènement clavier Slint.
pub fn combo_from_event(text: &str, ctrl: bool, alt: bool, shift: bool, meta: bool) -> Option<String> {
    use slint::platform::Key;
    let mut chars = text.chars();
    let c = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    let specials: [(Key, &str); 21] = [
        (Key::F1, "F1"), (Key::F2, "F2"), (Key::F3, "F3"), (Key::F4, "F4"), (Key::F5, "F5"), (Key::F6, "F6"),
        (Key::F7, "F7"), (Key::F8, "F8"), (Key::F9, "F9"), (Key::F10, "F10"), (Key::F11, "F11"), (Key::F12, "F12"),
        (Key::Home, "Home"), (Key::End, "End"), (Key::PageUp, "PageUp"), (Key::PageDown, "PageDown"),
        (Key::Delete, "Delete"), (Key::Insert, "Insert"), (Key::Space, "Space"), (Key::Tab, "Tab"), (Key::Return, "Enter"),
    ];
    let key = if let Some((_, n)) = specials.iter().find(|(k, _)| char::from(*k) == c) {
        n.to_string()
    } else if matches!(c, '\u{8}' | '\u{d}' | '\u{10}'..='\u{19}') {
        // Touches de modification seules (Maj, Ctrl, Alt, AltGr, Verr. Maj, Méta) et retour arrière.
        return None;
    } else if matches!(c, '\u{1}'..='\u{1a}') {
        // Ctrl+lettre produit un caractère de contrôle sur certains systèmes.
        ((b'A' + (c as u8) - 1) as char).to_string()
    } else if c.is_control() || ('\u{f700}'..='\u{f8ff}').contains(&c) || c == '\u{1b}' {
        return None;
    } else {
        // Clavier AZERTY : Ctrl+1…9 donne « & é " ' ( - è _ ç ».
        let azerty = ['&', 'é', '"', '\'', '(', '-', 'è', '_', 'ç', 'à'];
        match azerty.iter().position(|a| *a == c) {
            Some(i) if ctrl && !shift => ((i as u8 + 1) % 10).to_string(),
            _ => c.to_uppercase().to_string(),
        }
    };
    let mut parts: Vec<&str> = Vec::new();
    if ctrl {
        parts.push("Ctrl");
    }
    if alt {
        parts.push("Alt");
    }
    // Maj fait déjà partie des caractères ponctuation/majuscules : on ne l'ajoute qu'aux touches nommées et lettres.
    if shift && (key.len() > 1 || key.chars().all(|c| c.is_ascii_alphanumeric())) {
        parts.push("Shift");
    }
    if meta {
        parts.push("Super");
    }
    let mut out = parts.join("+");
    if !out.is_empty() {
        out.push('+');
    }
    out.push_str(&key);
    Some(out)
}

/// Normalise une combinaison saisie à la main (« ctrl + k » → « Ctrl+K »).
pub fn normalize_combo(s: &str) -> Option<String> {
    let mut mods: Vec<&str> = Vec::new();
    let mut key: Option<String> = None;
    for part in s.split('+').map(str::trim).filter(|p| !p.is_empty()) {
        match part.to_lowercase().as_str() {
            "ctrl" | "control" | "contrôle" => mods.push("Ctrl"),
            "alt" => mods.push("Alt"),
            "shift" | "maj" => mods.push("Shift"),
            "super" | "win" | "meta" => mods.push("Super"),
            _ if key.is_none() => {
                key = Some(if part.len() == 1 { part.to_uppercase() } else { part[..1].to_uppercase() + &part[1..] });
            }
            _ => return None,
        }
    }
    if s.trim_end().ends_with("++") || s.trim() == "+" {
        key = Some("+".into());
    }
    let key = key?;
    let order = ["Ctrl", "Alt", "Shift", "Super"];
    let mut out: Vec<String> = order.iter().filter(|m| mods.contains(m)).map(|m| m.to_string()).collect();
    out.push(key);
    Some(out.join("+"))
}

impl Controller {
    // ------------------------------------------------------------------ raccourcis

    pub(crate) fn shortcut_combo(&self, action: &str) -> String {
        let s = self.settings.borrow();
        s.shortcuts
            .get(action)
            .cloned()
            .unwrap_or_else(|| SHORTCUTS.iter().find(|x| x.0 == action).map(|x| x.2.to_string()).unwrap_or_default())
    }

    /// Évènement clavier non traité par l'interface : `true` s'il correspond à un raccourci.
    pub(crate) fn on_shortcut(self: &Rc<Self>, text: &str, ctrl: bool, alt: bool, shift: bool, meta: bool) -> bool {
        let Some(combo) = combo_from_event(text, ctrl, alt, shift, meta) else { return false };
        if ctrl && !alt && !meta && !shift {
            if let Some(n) = combo.strip_prefix("Ctrl+").and_then(|d| d.parse::<usize>().ok()).filter(|n| (1..=9).contains(n)) {
                if let Some(app) = self.app() {
                    app.global::<Nav>().invoke_go(NAV_PAGES[n - 1]);
                }
                return true;
            }
        }
        let action = SHORTCUTS.iter().map(|x| x.0).find(|a| self.shortcut_combo(a).eq_ignore_ascii_case(&combo));
        let Some(action) = action else { return false };
        self.run_action(action);
        true
    }

    fn run_action(self: &Rc<Self>, action: &str) {
        let Some(app) = self.app() else { return };
        match action {
            "quick_launch" => self.open_quick_launch(),
            "search" => {
                let ui = app.global::<SystemUi>();
                ui.set_focus_search_tick(ui.get_focus_search_tick() + 1);
            }
            "big_picture" => self.enter_big_picture(),
            "mini_mode" => self.show_mini(),
            "rescan" => self.start_scan(),
            "surprise" => self.surprise(),
            "settings" => app.global::<Nav>().invoke_go(Page::Settings),
            _ => {}
        }
    }

    pub(crate) fn set_shortcut(&self, action: &str, combo: &str) {
        let combo = combo.trim();
        let Some(def) = SHORTCUTS.iter().find(|x| x.0 == action) else { return };
        if combo.is_empty() {
            self.settings.borrow_mut().shortcuts.insert(action.to_string(), String::new());
        } else {
            let Some(c) = normalize_combo(combo) else {
                self.toast(tr!("Combinaison « {combo} » invalide", combo = combo), true);
                return;
            };
            if let Some(other) = SHORTCUTS.iter().find(|x| x.0 != action && self.shortcut_combo(x.0).eq_ignore_ascii_case(&c)) {
                self.toast(tr!("{c} est déjà utilisé par « {v} »", c = c, v = halcyon_i18n::t(other.1)), true);
                self.refresh_system_ui();
                return;
            }
            if c == def.2 {
                self.settings.borrow_mut().shortcuts.remove(action);
            } else {
                self.settings.borrow_mut().shortcuts.insert(action.to_string(), c);
            }
        }
        self.save_settings();
        self.refresh_system_ui();
    }

    pub(crate) fn reset_shortcuts(&self) {
        self.settings.borrow_mut().shortcuts.clear();
        self.save_settings();
        self.refresh_system_ui();
        self.toast(tr!("Raccourcis par défaut rétablis"), false);
    }

    pub(crate) fn refresh_system_ui(&self) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<SystemUi>();
        let s = self.settings.borrow().clone();
        ui.set_autostart(s.autostart);
        ui.set_close_to_tray(s.close_to_tray);
        ui.set_minimize_to_tray(s.minimize_to_tray);
        ui.set_global_enabled(s.global_hotkey_enabled);
        ui.set_global_hotkey(s.global_hotkey.clone().into());
        ui.set_companion(s.companion_window);
        let rows: Vec<ShortcutRow> = SHORTCUTS
            .iter()
            .map(|(a, l, d)| ShortcutRow { action: (*a).into(), label: halcyon_i18n::t(l).into(), combo: self.shortcut_combo(a).into(), default_combo: (*d).into() })
            .collect();
        ui.set_shortcuts(ModelRc::new(VecModel::from(rows)));
        ui.set_tray_available(self.tray.borrow().is_some());
    }

    // ------------------------------------------------------------------ options système

    pub(crate) fn set_system_option(self: &Rc<Self>, key: &str, value: bool) {
        match key {
            "autostart" => match crate::desktop::set_autostart(value) {
                Ok(()) => self.settings.borrow_mut().autostart = value,
                Err(e) => self.toast(tr!("Démarrage automatique : {e}", e = e), true),
            },
            "close_to_tray" => self.settings.borrow_mut().close_to_tray = value,
            "minimize_to_tray" => self.settings.borrow_mut().minimize_to_tray = value,
            "companion" => {
                self.settings.borrow_mut().companion_window = value;
                if !value {
                    self.close_companion();
                }
            }
            "global" => {
                self.settings.borrow_mut().global_hotkey_enabled = value;
                self.save_settings();
                self.register_global_hotkey();
            }
            _ => return,
        }
        self.save_settings();
        if matches!(key, "autostart" | "close_to_tray" | "minimize_to_tray") {
            self.ensure_tray();
        }
        self.start_system_timer();
        self.refresh_system_ui();
    }

    pub(crate) fn set_global_hotkey(self: &Rc<Self>, combo: &str) {
        let Some(c) = normalize_combo(combo) else {
            self.system_status_global(&tr!("Combinaison invalide (ex. Ctrl+Alt+H)"));
            return;
        };
        self.settings.borrow_mut().global_hotkey = c;
        self.save_settings();
        self.register_global_hotkey();
        self.start_system_timer();
        self.refresh_system_ui();
    }

    fn system_status_global(&self, msg: &str) {
        if let Some(app) = self.app() {
            app.global::<SystemUi>().set_global_status(msg.into());
        }
    }

    /// (Ré)enregistre le raccourci global. Sous Wayland, les raccourcis globaux ne sont
    /// pas accessibles aux applications : un message l'indique.
    pub(crate) fn register_global_hotkey(&self) {
        use global_hotkey::{hotkey::HotKey, GlobalHotKeyManager};
        if let Some((manager, hk)) = self.hotkey.borrow_mut().take() {
            let _ = manager.unregister(hk);
        }
        let (enabled, combo) = {
            let s = self.settings.borrow();
            (s.global_hotkey_enabled, s.global_hotkey.clone())
        };
        if !enabled {
            self.system_status_global("");
            return;
        }
        let hk: HotKey = match combo.parse() {
            Ok(h) => h,
            Err(e) => return self.system_status_global(&tr!("Combinaison non reconnue : {e}", e = e)),
        };
        if cfg!(target_os = "linux") && std::env::var_os("WAYLAND_DISPLAY").is_some() && std::env::var_os("DISPLAY").is_none() {
            return self.system_status_global(&tr!(
                "Session Wayland : les raccourcis globaux ne sont pas accessibles aux applications. Associez « halcyon --quick » à un raccourci dans les réglages de votre bureau."
            ));
        }
        match GlobalHotKeyManager::new().and_then(|m| m.register(hk).map(|_| m)) {
            Ok(m) => {
                *self.hotkey.borrow_mut() = Some((m, hk));
                self.system_status_global(&tr!("{combo} ouvre le lancement rapide, même quand Halcyon est en arrière-plan", combo = combo));
            }
            Err(e) => self.system_status_global(&tr!("Raccourci indisponible (déjà utilisé ?) : {e}", e = e)),
        }
    }

    // ------------------------------------------------------------------ zone de notification

    pub(crate) fn tray_wanted(&self) -> bool {
        let s = self.settings.borrow();
        s.close_to_tray || s.minimize_to_tray || s.autostart
    }

    /// Crée l'icône si une option la demande (et la retire sinon).
    pub(crate) fn ensure_tray(&self) {
        if !self.tray_wanted() {
            *self.tray.borrow_mut() = None;
            self.set_tray_status("");
            return;
        }
        if self.tray.borrow().is_some() {
            return;
        }
        match crate::tray::create(|ev| post(move |c| c.on_tray(ev))) {
            Ok(t) => {
                *self.tray.borrow_mut() = Some(t);
                self.set_tray_status(&tr!("Icône active dans la zone de notification"));
            }
            Err(e) => {
                log::warn!("{e}");
                self.set_tray_status(&tr!("{e} : la fenêtre sera réduite au lieu d'être masquée", e = e));
            }
        }
    }

    fn set_tray_status(&self, msg: &str) {
        if let Some(app) = self.app() {
            app.global::<SystemUi>().set_tray_status(msg.into());
            app.global::<SystemUi>().set_tray_available(self.tray.borrow().is_some());
        }
    }

    pub(crate) fn on_tray(self: &Rc<Self>, ev: TrayEvent) {
        match ev {
            TrayEvent::Show => self.show_main(),
            TrayEvent::QuickLaunch => {
                self.show_main();
                self.open_quick_launch();
            }
            TrayEvent::Mini => self.show_mini(),
            TrayEvent::BigPicture => {
                self.show_main();
                self.enter_big_picture();
            }
            TrayEvent::Quit => self.quit(),
        }
    }

    /// Relève les évènements de l'icône (Windows) et du raccourci global ; masque la
    /// fenêtre réduite si l'option est active.
    pub(crate) fn start_system_timer(self: &Rc<Self>) {
        // Rien à relever sans icône (Windows) ni raccourci global : aucun réveil inutile.
        let needed = (cfg!(windows) && self.tray.borrow().is_some()) || self.hotkey.borrow().is_some() || (self.settings.borrow().minimize_to_tray && self.tray.borrow().is_some());
        if !needed {
            *self.system_timer.borrow_mut() = None;
            return;
        }
        if self.system_timer.borrow().is_some() {
            return;
        }
        let weak = Rc::downgrade(self);
        let timer = slint::Timer::default();
        timer.start(slint::TimerMode::Repeated, Duration::from_millis(250), move || {
            let Some(c) = weak.upgrade() else { return };
            let events = c.tray.borrow().as_ref().map(Tray::poll).unwrap_or_default();
            for ev in events {
                c.on_tray(ev);
            }
            while let Ok(ev) = global_hotkey::GlobalHotKeyEvent::receiver().try_recv() {
                if ev.state == global_hotkey::HotKeyState::Pressed {
                    c.show_main();
                    c.open_quick_launch();
                }
            }
            if c.settings.borrow().minimize_to_tray && c.tray.borrow().is_some() && c.active_session.get().is_none() {
                c.hide_if_minimized();
            }
        });
        *self.system_timer.borrow_mut() = Some(timer);
    }

    fn hide_if_minimized(&self) {
        use slint::winit_030::WinitWindowAccessor;
        let Some(app) = self.app() else { return };
        let minimized = app.window().with_winit_window(|w| w.is_minimized().unwrap_or(false)).unwrap_or(false);
        if minimized && app.window().is_visible() {
            let _ = app.hide();
        }
    }

    pub(crate) fn show_main(&self) {
        use slint::winit_030::WinitWindowAccessor;
        let Some(app) = self.app() else { return };
        if let Some(m) = self.mini.borrow().as_ref() {
            let _ = m.hide();
        }
        let _ = app.show();
        app.window().with_winit_window(|w| {
            w.set_minimized(false);
            w.focus_window();
        });
    }

    /// Fermeture de la fenêtre : zone de notification ou arrêt complet.
    pub(crate) fn on_close_requested(&self) -> slint::CloseRequestResponse {
        self.save_window_state();
        if self.settings.borrow().close_to_tray && self.tray.borrow().is_some() {
            return slint::CloseRequestResponse::HideWindow;
        }
        self.quit();
        slint::CloseRequestResponse::HideWindow
    }

    pub(crate) fn quit(&self) {
        self.audio.shutdown();
        self.save_window_state();
        crate::instance::release(&self.paths.data_dir);
        if let Some(m) = self.mini.borrow().as_ref() {
            let _ = m.hide();
        }
        self.close_companion();
        let _ = slint::quit_event_loop();
    }

    // ------------------------------------------------------------------ instance unique

    pub(crate) fn on_instance_command(self: &Rc<Self>, cmd: crate::instance::Command) {
        use crate::instance::Command;
        match cmd {
            Command::Show => self.show_main(),
            Command::Quick => {
                self.show_main();
                self.open_quick_launch();
            }
            Command::BigPicture => {
                self.show_main();
                self.enter_big_picture();
            }
            Command::Launch(id) => self.play(id),
            Command::Open(p) => {
                self.show_main();
                self.open_path(&p);
            }
        }
    }

    /// Fichier ouvert avec Halcyon (association) : thème ou extension.
    pub(crate) fn open_path(self: &Rc<Self>, p: &std::path::Path) {
        let name = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        if let Some(app) = self.app() {
            if name.ends_with(".halcyontheme") || name.ends_with(".json") && !name.ends_with("plugin.json") {
                app.global::<Nav>().invoke_go(Page::Appearance);
                self.import_theme(&p.display().to_string());
            } else {
                app.global::<Nav>().invoke_go(Page::Plugins);
                self.install_plugin(&p.display().to_string());
            }
        }
    }

    // ------------------------------------------------------------------ lancement rapide

    pub(crate) fn open_quick_launch(&self) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<QuickUi>();
        ui.set_query("".into());
        ui.set_selected(0);
        ui.set_open(true);
        self.quick_search("");
    }

    pub(crate) fn quick_search(&self, q: &str) {
        let Some(app) = self.app() else { return };
        let now = now_unix();
        let lib = self.lib.borrow();
        let mut rows: Vec<QuickRow> = Vec::new();
        let game_row = |g: &halcyon_core::model::Game| QuickRow {
            kind: 0,
            id: g.id as i32,
            title: g.name.clone().into(),
            subtitle: if g.installed { g.source.label() } else { tr!("{source} · non installé", source = g.source.label()) }.into(),
            card: card_for(g, &self.images, now),
        };
        if q.trim().is_empty() {
            let mut recent: Vec<_> = lib.games.iter().filter(|g| !g.hidden && g.last_played.is_some()).collect();
            recent.sort_by_key(|g| std::cmp::Reverse(g.last_played));
            rows.extend(recent.into_iter().take(8).map(game_row));
        } else {
            let query = SearchQuery::parse(q);
            let ctx = lib.ctx.clone();
            let mut found: Vec<(MatchTier, &halcyon_core::model::Game)> =
                lib.games.iter().filter(|g| !g.hidden).filter_map(|g| query.matches(g, &ctx).map(|t| (t, g))).collect();
            found.sort_by(|a, b| {
                (a.0 != MatchTier::Direct)
                    .cmp(&(b.0 != MatchTier::Direct))
                    .then(b.1.installed.cmp(&a.1.installed))
                    .then(b.1.last_played.cmp(&a.1.last_played))
                    .then(a.1.sort_key.cmp(&b.1.sort_key))
            });
            rows.extend(found.into_iter().take(7).map(|(_, g)| game_row(g)));
            let key = halcyon_core::util::search_key(q);
            for (i, (label, words, _)) in QUICK_PAGES.iter().enumerate() {
                let hay = halcyon_core::util::search_key(&crate::ctl_palette::search_text(label, words));
                if key.split(' ').filter(|w| !w.is_empty()).all(|w| hay.contains(w)) {
                    rows.push(QuickRow { kind: 1, id: i as i32, title: halcyon_i18n::t(label).into(), subtitle: tr!("Page").into(), card: Default::default() });
                }
            }
        }
        drop(lib);
        rows.extend(self.palette_rows(q));
        rows.truncate(12);
        let ui = app.global::<QuickUi>();
        if ui.get_selected() >= rows.len() as i32 {
            ui.set_selected(0);
        }
        ui.set_rows(ModelRc::new(VecModel::from(rows)));
    }

    pub(crate) fn quick_activate(self: &Rc<Self>, index: usize, open_detail: bool) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<QuickUi>();
        let Some(row) = slint::Model::row_data(&ui.get_rows(), index) else { return };
        ui.set_open(false);
        if row.kind >= 2 {
            self.palette_activate(row.kind, row.id);
        } else if row.kind == 1 {
            if let Some((_, _, page)) = QUICK_PAGES.get(row.id as usize) {
                app.global::<Nav>().invoke_go(*page);
            }
        } else if open_detail {
            self.open_game(row.id as GameId);
        } else {
            self.play(row.id as GameId);
        }
    }

    // ------------------------------------------------------------------ mode compact

    pub(crate) fn show_mini(self: &Rc<Self>) {
        if self.mini.borrow().is_none() {
            let w = match MiniWindow::new() {
                Ok(w) => w,
                Err(e) => return self.toast(tr!("Mode compact indisponible : {e}", e = e), true),
            };
            let weak = Rc::downgrade(self);
            let wk = weak.clone();
            w.on_search(move |q| {
                if let Some(c) = wk.upgrade() {
                    c.refresh_mini(&q);
                }
            });
            let wk = weak.clone();
            w.on_launch(move |id| {
                if let Some(c) = wk.upgrade() {
                    c.play(id as GameId);
                }
            });
            let wk = weak.clone();
            w.on_open_main(move || {
                if let Some(c) = wk.upgrade() {
                    c.show_main();
                }
            });
            let wk = weak.clone();
            w.on_toggle_on_top(move || {
                if let Some(c) = wk.upgrade() {
                    if let Some(m) = c.mini.borrow().as_ref() {
                        m.set_on_top(!m.get_on_top());
                    }
                }
            });
            let wk = weak.clone();
            w.window().on_close_requested(move || {
                if let Some(c) = wk.upgrade() {
                    c.show_main();
                }
                slint::CloseRequestResponse::HideWindow
            });
            *self.mini.borrow_mut() = Some(w);
            self.apply_theme_to_windows();
        }
        self.refresh_mini("");
        if let Some(m) = self.mini.borrow().as_ref() {
            m.set_query("".into());
            let _ = m.show();
        }
        if let Some(app) = self.app() {
            let _ = app.hide();
        }
    }

    fn mini_row(g: &halcyon_core::model::Game, now: i64) -> MiniRow {
        let sub = match g.last_played {
            Some(t) => format!("{} · {}", g.source.label(), halcyon_core::util::format_relative(t, now)),
            None => g.source.label().to_string(),
        };
        MiniRow { id: g.id as i32, title: g.name.clone().into(), subtitle: sub.into(), installed: g.installed }
    }

    pub(crate) fn refresh_mini(&self, q: &str) {
        let mini = self.mini.borrow();
        let Some(m) = mini.as_ref() else { return };
        let lib = self.lib.borrow();
        let now = now_unix();
        let mut favs: Vec<_> = lib.games.iter().filter(|g| g.favorite && !g.hidden).collect();
        favs.sort_by(|a, b| a.sort_key.cmp(&b.sort_key));
        let mut recent: Vec<_> = lib.games.iter().filter(|g| !g.hidden && g.last_played.is_some() && !g.favorite).collect();
        recent.sort_by_key(|g| std::cmp::Reverse(g.last_played));
        m.set_favorites(ModelRc::new(VecModel::from(favs.iter().take(12).map(|g| Self::mini_row(g, now)).collect::<Vec<_>>())));
        m.set_recent(ModelRc::new(VecModel::from(recent.iter().take(10).map(|g| Self::mini_row(g, now)).collect::<Vec<_>>())));
        if !q.trim().is_empty() {
            let query = SearchQuery::parse(q);
            let ctx = lib.ctx.clone();
            let mut found: Vec<_> = lib.games.iter().filter(|g| !g.hidden).filter_map(|g| query.matches(g, &ctx).map(|t| (t, g))).collect();
            found.sort_by(|a, b| (a.0 != MatchTier::Direct).cmp(&(b.0 != MatchTier::Direct)).then(b.1.installed.cmp(&a.1.installed)));
            m.set_results(ModelRc::new(VecModel::from(found.iter().take(12).map(|(_, g)| Self::mini_row(g, now)).collect::<Vec<_>>())));
        }
    }

    // ------------------------------------------------------------------ fenêtre compagnon

    pub(crate) fn open_companion(self: &Rc<Self>, id: GameId) {
        if !self.settings.borrow().companion_window {
            return;
        }
        let Some(g) = self.lib.borrow().get(id).cloned() else { return };
        if self.companion.borrow().is_none() {
            let w = match CompanionWindow::new() {
                Ok(w) => w,
                Err(e) => return log::warn!("fenêtre compagnon : {e}"),
            };
            let weak = Rc::downgrade(self);
            let wk = weak.clone();
            w.on_save_notes(move |notes| {
                if let Some(c) = wk.upgrade() {
                    let id = c.companion_game.get();
                    if let Some(id) = id {
                        c.save_notes(id, &notes);
                        if let Some(w) = c.companion.borrow().as_ref() {
                            w.set_status(tr!("Notes enregistrées").into());
                        }
                    }
                }
            });
            let wk = weak.clone();
            w.on_hide_window(move || {
                if let Some(c) = wk.upgrade() {
                    c.close_companion();
                }
            });
            *self.companion.borrow_mut() = Some(w);
            self.apply_theme_to_windows();
        }
        self.companion_game.set(Some(id));
        let start = now_unix();
        let base = self.progress.borrow().get(&id).map(|p| p.unlocked).unwrap_or(0);
        if let Some(w) = self.companion.borrow().as_ref() {
            w.set_game(g.name.clone().into());
            w.set_notes(g.notes.clone().into());
            w.set_status("".into());
            w.set_elapsed("0:00".into());
            w.set_achievements("".into());
            let _ = w.show();
        }
        let weak = Rc::downgrade(self);
        let timer = slint::Timer::default();
        timer.start(slint::TimerMode::Repeated, Duration::from_secs(1), move || {
            let Some(c) = weak.upgrade() else { return };
            let secs = (now_unix() - start).max(0);
            let elapsed = if secs >= 3600 {
                format!("{}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
            } else {
                format!("{}:{:02}", secs / 60, secs % 60)
            };
            let ach = c.progress.borrow().get(&id).map(|p| {
                let new = p.unlocked.saturating_sub(base);
                if new > 0 { tr!("Succès : {unlocked} / {total} (+{new} pendant cette session)", unlocked = p.unlocked, total = p.total, new = new) } else { tr!("Succès : {unlocked} / {total}", unlocked = p.unlocked, total = p.total) }
            });
            let companion = c.companion.borrow();
            if let Some(w) = companion.as_ref() {
                w.set_elapsed(elapsed.into());
                w.set_achievements(ach.unwrap_or_default().into());
            }
        });
        *self.companion_timer.borrow_mut() = Some(timer);
    }

    pub(crate) fn close_companion(&self) {
        *self.companion_timer.borrow_mut() = None;
        self.companion_game.set(None);
        if let Some(w) = self.companion.borrow().as_ref() {
            let _ = w.hide();
        }
    }

    /// Recopie les couleurs du thème dans les fenêtres annexes (chaque fenêtre a ses globaux).
    pub(crate) fn apply_theme_to_windows(&self) {
        let Some(app) = self.app() else { return };
        let t = app.global::<crate::Theme>();
        macro_rules! copy {
            ($w:expr) => {{
                let d = $w.global::<crate::Theme>();
                d.set_bg(t.get_bg());
                d.set_sidebar(t.get_sidebar());
                d.set_surface(t.get_surface());
                d.set_surface_hover(t.get_surface_hover());
                d.set_surface_active(t.get_surface_active());
                d.set_border(t.get_border());
                d.set_border_strong(t.get_border_strong());
                d.set_text(t.get_text());
                d.set_text_muted(t.get_text_muted());
                d.set_text_dim(t.get_text_dim());
                d.set_accent(t.get_accent());
                d.set_on_accent(t.get_on_accent());
                d.set_warm(t.get_warm());
                d.set_danger(t.get_danger());
                d.set_success(t.get_success());
                d.set_animations(t.get_animations());
            }};
        }
        if let Some(m) = self.mini.borrow().as_ref() {
            copy!(m);
        }
        if let Some(w) = self.companion.borrow().as_ref() {
            copy!(w);
        }
    }

    // ------------------------------------------------------------------ raccourci de jeu

    pub(crate) fn create_game_shortcut(&self, id: GameId) {
        let Some(g) = self.lib.borrow().get(id).cloned() else { return };
        let icon = g.artwork.cover.clone();
        match crate::desktop::create_game_shortcut(&g.name, g.id, icon.as_deref()) {
            Ok(p) => self.toast(tr!("Raccourci créé : {p}", p = p.display()), false),
            Err(e) => self.toast(tr!("Raccourci impossible : {e}", e = e), true),
        }
    }

    // ------------------------------------------------------------------ intégrité

    /// Vérifie la base, les paramètres, les dossiers de données, les visuels et les extensions.
    pub(crate) fn check_integrity(&self) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<SystemUi>();
        ui.set_integrity_running(true);
        let mut lines: Vec<String> = Vec::new();
        let mut problems = 0;
        let db_result = self.db().integrity_check();
        match db_result {
            Ok(None) => lines.push(tr!("Base de données : intègre").into()),
            Ok(Some(e)) => {
                problems += 1;
                lines.push(tr!("Base de données : problèmes détectés ({e}). Exportez la configuration puis restaurez-la pour reconstruire la base.", e = e));
            }
            Err(e) => {
                problems += 1;
                lines.push(tr!("Base de données : vérification impossible ({e})", e = e));
            }
        }
        match std::fs::read_to_string(self.paths.settings_path()).map(|t| serde_json::from_str::<serde_json::Value>(&t)) {
            Ok(Ok(_)) => lines.push(tr!("Paramètres : lisibles").into()),
            Ok(Err(e)) => {
                problems += 1;
                lines.push(tr!("Paramètres : fichier invalide ({e})", e = e));
            }
            Err(_) => lines.push(tr!("Paramètres : valeurs par défaut (aucun fichier)").into()),
        }
        let probe = self.paths.data_dir.join(".integrity-test");
        match std::fs::write(&probe, b"ok").and_then(|_| std::fs::remove_file(&probe)) {
            Ok(()) => lines.push(tr!("Dossier des données : accessible en écriture").into()),
            Err(e) => {
                problems += 1;
                lines.push(tr!("Dossier des données : écriture impossible ({e})", e = e));
            }
        }
        let (missing_dirs, missing_art) = {
            let lib = self.lib.borrow();
            let dirs = lib.games.iter().filter(|g| g.installed && g.install_dir.as_ref().is_some_and(|d| !d.exists())).count();
            let art = lib.games.iter().filter(|g| g.artwork.cover.as_ref().is_some_and(|p| !p.is_file())).count();
            (dirs, art)
        };
        if missing_dirs > 0 {
            lines.push(trn!(missing_dirs, "{n} jeu marqué installé dont le dossier a disparu : relancez une analyse des sources", "{n} jeux marqués installés dont le dossier a disparu : relancez une analyse des sources"));
        } else {
            lines.push(tr!("Dossiers des jeux installés : présents").into());
        }
        if missing_art > 0 {
            lines.push(trn!(missing_art, "{n} jaquette manquante : elle sera retéléchargée si les visuels en ligne sont activés", "{n} jaquettes manquantes : elles seront retéléchargées si les visuels en ligne sont activés"));
        }
        let bad_plugins: Vec<String> = halcyon_integrations::plugins::list(&self.plugins_dir(), crate::ctl_plugins::VERSION)
            .into_iter()
            .filter(|p| !p.usable())
            .map(|p| p.id())
            .collect();
        if !bad_plugins.is_empty() {
            problems += 1;
            lines.push(tr!("Extensions inutilisables : {list}", list = bad_plugins.join(", ")));
        }
        let summary = if problems == 0 { tr!("Aucun problème détecté") } else { trn!(problems, "{n} problème à corriger", "{n} problèmes à corriger") };
        let logged = if problems == 0 { tr_later!("Aucun problème détecté") } else { trn_later!(problems, "{n} problème à corriger", "{n} problèmes à corriger") };
        lines.insert(0, summary.clone());
        ui.set_integrity_status(lines.join("\n").into());
        ui.set_integrity_running(false);
        let _ = self.db().log_activity(halcyon_storage::ActivityKind::Settings, None, &tr_later!("Vérification d'intégrité"), &logged);
    }

    pub(crate) fn wire_system(self: &Rc<Self>, app: &crate::AppWindow) {
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
        let ui = app.global::<SystemUi>();
        let wk = weak.clone();
        ui.on_shortcut(move |t, c, a, s, m| wk.upgrade().is_some_and(|ctl| ctl.on_shortcut(&t, c, a, s, m)));
        ui.on_combo_of(|t, c, a, s, m| SharedString::from(combo_from_event(&t, c, a, s, m).unwrap_or_default()));
        ui.on_set_option(with!(|c, k, v| c.set_system_option(&k, v)));
        ui.on_set_shortcut(with!(|c, a, combo| c.set_shortcut(&a, &combo)));
        ui.on_reset_shortcuts(with!(|c| c.reset_shortcuts()));
        ui.on_set_global_hotkey(with!(|c, combo| c.set_global_hotkey(&combo)));
        ui.on_check_integrity(with!(|c| c.check_integrity()));
        ui.on_create_shortcut(with!(|c, id| c.create_game_shortcut(id as GameId)));
        ui.on_mini_mode(with!(|c| c.show_mini()));
        let q = app.global::<QuickUi>();
        q.on_query_changed(with!(|c, t| c.quick_search(&t)));
        q.on_activate(with!(|c, i, d| c.quick_activate(i.max(0) as usize, d)));
        let _ = (app.global::<DetailUi>(), app.global::<AppUi>(), app.global::<SettingsUi>());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slint::platform::Key;

    #[test]
    fn combos() {
        assert_eq!(combo_from_event("k", true, false, false, false).as_deref(), Some("Ctrl+K"));
        assert_eq!(combo_from_event("\u{b}", true, false, false, false).as_deref(), Some("Ctrl+K"));
        assert_eq!(combo_from_event(&char::from(Key::F5).to_string(), false, false, false, false).as_deref(), Some("F5"));
        assert_eq!(combo_from_event("&", true, false, false, false).as_deref(), Some("Ctrl+1"));
        assert_eq!(combo_from_event("K", true, false, true, false).as_deref(), Some("Ctrl+Shift+K"));
        assert_eq!(combo_from_event("", true, false, false, false), None);
        assert_eq!(combo_from_event(&char::from(Key::Control).to_string(), true, false, false, false), None);
        assert_eq!(combo_from_event(&char::from(Key::Shift).to_string(), false, false, true, false), None);
        assert_eq!(normalize_combo("ctrl + alt + h").as_deref(), Some("Ctrl+Alt+H"));
        assert_eq!(normalize_combo("maj+f5").as_deref(), Some("Shift+F5"));
        assert_eq!(normalize_combo("Ctrl+A+B"), None);
    }
}

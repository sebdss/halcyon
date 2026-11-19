//! Coque de l'interface 0.4 : historique de navigation, modes (standard, minimaliste,
//! cinéma), mode Performance, centre de commandes, réglages de mouvement et de texte.

use crate::controller::Controller;
use crate::nav::Entry;
use crate::tasks::{Action, State};
use crate::gamepad::PadAction;
use crate::{AppUi, CommandUi, DetailUi, GameMenu, LibraryUi, Nav, Page, QuickUi, TaskRow, Theme};
use halcyon_core::model::GameId;
use halcyon_storage::now_unix;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::rc::Rc;

fn page_from_name(name: &str) -> Option<Page> {
    Some(match name {
        "home" => Page::Home,
        "library" => Page::Library,
        "updates" => Page::Updates,
        "saves" => Page::Saves,
        "settings" => Page::Settings,
        "achievements" => Page::Achievements,
        "journal" => Page::Journal,
        "plugins" => Page::Plugins,
        "emulators" => Page::Emulators,
        "minecraft" => Page::Minecraft,
        "performance" => Page::Performance,
        "downloads" => Page::Downloads,
        _ => return None,
    })
}

fn clock(t: i64) -> String {
    let local = t + crate::ctl_bigpicture::local_offset_secs();
    let s = local.rem_euclid(86_400);
    format!("{:02}:{:02}", s / 3600, s / 60 % 60)
}

impl Controller {
    // ------------------------------------------------------------------ historique

    /// Page visitée depuis l'interface (`Nav.go`).
    pub(crate) fn on_navigated(&self, page: Page) {
        self.sound(halcyon_audio::Cue::Section);
        let game = (page == Page::Game).then(|| self.detail_id.get()).flatten();
        self.history.borrow_mut().push(Entry { page, game });
        self.publish_history();
    }

    pub(crate) fn push_game_history(&self, id: GameId) {
        self.history.borrow_mut().push(Entry { page: Page::Game, game: Some(id) });
        self.publish_history();
    }

    pub(crate) fn on_history(&self, delta: i32) {
        let Some(app) = self.app() else { return };
        let target = self.history.borrow_mut().go(delta);
        let Some(e) = target else { return };
        self.sound(if delta < 0 { halcyon_audio::Cue::Back } else { halcyon_audio::Cue::Section });
        if let (Page::Game, Some(id)) = (e.page, e.game) {
            if self.lib.borrow().get(id).is_some() {
                self.show_game(id);
            }
        } else if e.page != Page::Game {
            let nav = app.global::<Nav>();
            nav.set_return_page(e.page);
            nav.set_page(e.page);
        }
        self.publish_history();
    }

    fn publish_history(&self) {
        let Some(app) = self.app() else { return };
        let h = self.history.borrow();
        let nav = app.global::<Nav>();
        nav.set_can_back(h.can_back());
        nav.set_can_forward(h.can_forward());
    }

    // ------------------------------------------------------------------ modes

    /// Applique le mode d'interface, le mouvement et la taille du texte enregistrés.
    pub(crate) fn apply_ui_modes(&self) {
        let Some(app) = self.app() else { return };
        let (a, performance) = {
            let s = self.settings.borrow();
            (s.appearance.clone(), s.appearance.performance || self.perf_auto_active.get())
        };
        let ui = app.global::<AppUi>();
        ui.set_ui_mode(i32::from(a.ui_mode.min(2)));
        ui.set_sidebar_collapsed(a.sidebar_collapsed);
        ui.set_performance(performance);
        ui.set_performance_auto(self.perf_auto_active.get() && !a.performance);
        let theme = app.global::<Theme>();
        // Mode Performance : aucun mouvement, aucun effet coûteux, pas de fond animé.
        // Mode minimaliste : mouvement réduit.
        let motion = if performance { 2 } else if a.ui_mode == 1 { a.motion().max(1) } else { a.motion() };
        theme.set_motion(i32::from(motion));
        theme.set_animations(motion < 2);
        theme.set_effects(!performance);
        self.gl3d_redraw();
        theme.set_text_scale(a.text_scale.clamp(0.9, 1.3));
        if performance {
            theme.set_animated_background(false);
        }
        self.apply_theme_to_windows();
    }

    pub(crate) fn set_ui_mode(&self, mode: i32) {
        let mode = mode.clamp(0, 2) as u8;
        self.settings.borrow_mut().appearance.ui_mode = mode;
        self.save_settings();
        self.apply_ui_modes();
        self.sound(halcyon_audio::Cue::Mode);
        let label = match mode as usize { 1 => tr!("Mode minimaliste"), 2 => tr!("Mode cinéma"), _ => tr!("Mode standard") };
        self.toast(label, false);
        self.refresh_home();
    }

    pub(crate) fn set_sidebar_collapsed(&self, collapsed: bool) {
        self.settings.borrow_mut().appearance.sidebar_collapsed = collapsed;
        self.save_settings();
    }

    /// Mode Performance manuel.
    pub(crate) fn set_performance(&self, on: bool) {
        // Désactiver pendant une partie coupe aussi le mode automatique pour cette partie.
        if !on {
            self.perf_auto_active.set(false);
        }
        self.settings.borrow_mut().appearance.performance = on;
        self.save_settings();
        self.sound(if on { halcyon_audio::Cue::ToggleOn } else { halcyon_audio::Cue::ToggleOff });
        self.apply_ui_modes();
        self.on_performance_changed();
        self.toast(if on { tr!("Mode Performance activé") } else { tr!("Mode Performance désactivé") }, false);
    }

    /// Mode Performance actif (manuel ou automatique).
    pub(crate) fn performance_active(&self) -> bool {
        self.settings.borrow().appearance.performance || self.perf_auto_active.get()
    }

    /// Début d'une partie : mode Performance automatique si l'option est active.
    pub(crate) fn performance_session_started(&self) {
        self.audio.set_game_active(true);
        if let Some(id) = self.active_session.get() {
            self.perf_session_start(id);
        }
        if self.settings.borrow().appearance.performance_auto && !self.perf_auto_active.get() {
            self.perf_auto_active.set(true);
            self.apply_ui_modes();
            self.on_performance_changed();
        }
    }

    /// Fin de la partie : fonctionnement normal restauré.
    pub(crate) fn performance_session_ended(&self) {
        self.audio.set_game_active(false);
        self.perf_session_stop();
        if self.perf_auto_active.replace(false) {
            self.apply_ui_modes();
            self.on_performance_changed();
        }
    }

    /// Tâches secondaires suspendues ou reprises (synchronisations reportées, sondages ralentis).
    fn on_performance_changed(&self) {
        if !self.performance_active() {
            // Reprise : les téléchargements différés et la vérification des mises à jour repartent.
            self.start_online_jobs();
            crate::controller::post(|c| c.ensure_update_polling());
        }
    }

    // ------------------------------------------------------------------ manette (bureau)

    /// Envoie une touche à la fenêtre, comme le clavier : toute l'interface reste pilotable
    /// par les mêmes chemins (focus, raccourcis, menus), sans code dédié à la manette.
    fn send_key(&self, key: slint::platform::Key, shift: bool) {
        use slint::platform::{Key, WindowEvent};
        let Some(app) = self.app() else { return };
        let w = app.window();
        if shift {
            w.dispatch_event(WindowEvent::KeyPressed { text: Key::Shift.into() });
        }
        w.dispatch_event(WindowEvent::KeyPressed { text: key.into() });
        w.dispatch_event(WindowEvent::KeyReleased { text: key.into() });
        if shift {
            w.dispatch_event(WindowEvent::KeyReleased { text: Key::Shift.into() });
        }
    }

    /// Manette hors Big Picture : flèches dans les grilles et menus, sinon déplacement du
    /// focus (Tab) ; A valide, B revient, X/Y raccourcis, Select menu contextuel, LB/RB onglets.
    pub(crate) fn desktop_pad(self: &Rc<Self>, action: PadAction) {
        use slint::platform::Key;
        let Some(app) = self.app() else { return };
        let page = app.global::<Nav>().get_page();
        let menu_open = app.global::<GameMenu>().get_open() || app.global::<QuickUi>().get_open();
        let arrows = menu_open || page == Page::Library || page == Page::Collections;
        match action {
            PadAction::Up if arrows => self.send_key(Key::UpArrow, false),
            PadAction::Down if arrows => self.send_key(Key::DownArrow, false),
            PadAction::Left if arrows => self.send_key(Key::LeftArrow, false),
            PadAction::Right if arrows => self.send_key(Key::RightArrow, false),
            PadAction::Up | PadAction::Left => self.send_key(Key::Backtab, false),
            PadAction::Down | PadAction::Right => self.send_key(Key::Tab, false),
            PadAction::Accept => self.send_key(Key::Return, false),
            PadAction::Back => self.send_key(Key::Escape, false),
            PadAction::Context => self.send_key(Key::Menu, false),
            PadAction::Inspect => {}
            PadAction::Search => {
                let q = app.global::<QuickUi>();
                q.set_query("".into());
                q.set_open(true);
                q.invoke_query_changed("".into());
            }
            PadAction::Favorite => {
                let id = match page {
                    Page::Library => Some(app.global::<LibraryUi>().get_selected_id()),
                    Page::Game => Some(app.global::<DetailUi>().get_game().id),
                    _ => None,
                };
                if let Some(id) = id.filter(|i| *i >= 0) {
                    self.toggle_favorite(GameId::from(id));
                }
            }
            PadAction::Prev | PadAction::Next => {
                let d = if action == PadAction::Next { 1 } else { -1 };
                match page {
                    Page::Library => {
                        let l = app.global::<LibraryUi>();
                        l.set_view_mode((l.get_view_mode() + d).rem_euclid(5));
                        self.on_view_changed();
                    }
                    Page::Game => {
                        // Ordre affiché des onglets (voir ui/pages/game.slint).
                        const ORDER: [i32; 7] = [0, 2, 3, 7, 5, 6, 1];
                        let dui = app.global::<DetailUi>();
                        let cur = ORDER.iter().position(|t| *t == dui.get_tab()).unwrap_or(0) as i32;
                        dui.set_tab(ORDER[(cur + d).rem_euclid(ORDER.len() as i32) as usize]);
                        dui.invoke_tools_refresh();
                    }
                    _ => app.global::<Nav>().invoke_history(d),
                }
            }
            PadAction::Toggle if page == Page::Library && !menu_open => self.toggle_mark_selected(),
            PadAction::Menu | PadAction::Toggle => {}
        }
    }

    // ------------------------------------------------------------------ centre de commandes

    pub(crate) fn task_start(&self, kind: &'static str, title: impl Into<String>) -> u64 {
        let id = self.tasks.borrow_mut().start_unique(kind, title, now_unix());
        self.publish_tasks();
        id
    }

    pub(crate) fn task_update(&self, id: u64, progress: Option<f32>, detail: Option<&str>) {
        self.tasks.borrow_mut().update(id, progress, detail);
        self.publish_tasks();
    }

    pub(crate) fn task_finish(&self, id: u64, ok: bool, detail: impl Into<String>, action: Action) {
        self.tasks.borrow_mut().finish(id, ok, detail, action, now_unix());
        self.publish_tasks();
    }

    pub(crate) fn task_finish_kind(&self, kind: &str, ok: bool, detail: impl Into<String>) {
        let id = self.tasks.borrow().find_kind(kind);
        if let Some(id) = id {
            self.task_finish(id, ok, detail, Action::None);
        }
    }

    /// Notification ou erreur ponctuelle (aussi affichées en bandeau éphémère).
    pub(crate) fn task_event(&self, kind: &'static str, title: &str, detail: &str, error: bool, action: Action) {
        self.tasks.borrow_mut().event(kind, title, detail, error, action, now_unix());
        self.publish_tasks();
    }

    pub(crate) fn publish_tasks(&self) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<CommandUi>();
        let now = now_unix();
        let row = |t: &crate::tasks::Task| {
            let elapsed = now - t.started;
            TaskRow {
                id: t.id as i32,
                kind: t.kind.into(),
                title: t.title.clone().into(),
                detail: t.detail.clone().into(),
                progress: t.progress,
                state: t.state.code(),
                time: match t.state {
                    State::Running if elapsed >= 60 => tr!("{v} min", v = elapsed / 60),
                    State::Running => tr!("{elapsed} s", elapsed = elapsed),
                    _ => clock(t.ended.unwrap_or(t.started)),
                }
                .into(),
                action: match &t.action {
                    Action::None => SharedString::new(),
                    Action::OpenPage(_) => tr!("Ouvrir").into(),
                    Action::OpenGame(_) => tr!("Voir la fiche").into(),
                    Action::OpenFolder(_) => tr!("Ouvrir le dossier").into(),
                },
            }
        };
        let (active, history, unread, errors) = {
            let tasks = self.tasks.borrow();
            let active: Vec<TaskRow> = tasks.active().iter().map(row).collect();
            let history: Vec<TaskRow> = tasks.history().take(40).map(row).collect();
            (active, history, tasks.unread, tasks.errors())
        };
        ui.set_running(active.len() as i32);
        ui.set_errors(errors as i32);
        ui.set_unread(if ui.get_open() { 0 } else { unread as i32 });
        self.publish_download_rows(&active, &history);
        ui.set_active(ModelRc::new(VecModel::from(active)));
        ui.set_history(ModelRc::new(VecModel::from(history)));
    }

    pub(crate) fn on_command_center_opened(&self) {
        self.tasks.borrow_mut().unread = 0;
        self.publish_tasks();
    }

    pub(crate) fn on_task_action(&self, id: i32) {
        let action = self.tasks.borrow().get(id as u64).map(|t| t.action.clone());
        let Some(app) = self.app() else { return };
        match action {
            Some(Action::OpenPage(p)) => {
                if let Some(page) = page_from_name(p) {
                    app.global::<Nav>().invoke_go(page);
                }
            }
            Some(Action::OpenGame(g)) => self.open_game(g),
            Some(Action::OpenFolder(f)) => self.open_folder(&f),
            _ => {}
        }
        app.global::<CommandUi>().set_open(false);
    }

    pub(crate) fn wire_shell(self: &Rc<Self>, app: &crate::AppWindow) {
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
        let nav = app.global::<Nav>();
        nav.on_navigated(with!(|c, p| c.on_navigated(p)));
        nav.on_history(with!(|c, d| c.on_history(d)));
        let ui = app.global::<AppUi>();
        ui.on_set_ui_mode(with!(|c, m| c.set_ui_mode(m)));
        ui.on_set_performance(with!(|c, on| c.set_performance(on)));
        ui.on_sidebar_changed(with!(|c, v| c.set_sidebar_collapsed(v)));
        let cc = app.global::<CommandUi>();
        cc.on_opened(with!(|c| c.on_command_center_opened()));
        cc.on_task_action(with!(|c, id| c.on_task_action(id)));
        cc.on_clear_history(with!(|c| {
            c.tasks.borrow_mut().clear_history();
            c.publish_tasks();
        }));
        // L'heure affichée des tâches en cours avance seule (toutes les 5 s, seulement s'il y en a).
        let wk = weak.clone();
        let timer = slint::Timer::default();
        timer.start(slint::TimerMode::Repeated, std::time::Duration::from_secs(5), move || {
            if let Some(c) = wk.upgrade() {
                if !c.tasks.borrow().active().is_empty() {
                    c.publish_tasks();
                }
            }
        });
        *self.shell_timer.borrow_mut() = Some(timer);
    }
}

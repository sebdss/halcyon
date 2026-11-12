//! Programme d'installation de Halcyon.
//!
//! Le même exécutable sert à installer (charge utile ajoutée à sa fin), puis, copié sans sa
//! charge utile dans le dossier d'installation, à réparer et désinstaller.
//!
//! Ligne de commande :
//! - `--repair` : ouvre directement la réparation ;
//! - `--uninstall` : ouvre directement la désinstallation ;
//! - `--info` : affiche le contenu de la charge utile (manifeste) ;
//! - `--pack <dossier> <version> <sortie> [--min-glibc X.Y]` : construit un programme
//!   d'installation à partir d'un dossier (utilisé par les scripts de paquets).

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use halcyon_installer::engine::{self, Event, Found, Level, Mode, Origin, Options, Record, UninstallOptions};
use halcyon_installer::payload::{self, Payload};
use halcyon_installer::platform::{self, Env};
use slint::{ComponentHandle, Model, ModelRc, VecModel};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

slint::include_modules!();

const UNINSTALL_STEPS: [&str; 3] = ["Raccourcis et entrées", "Fichiers du programme", "Cache et données"];

struct State {
    env: Env,
    exe: PathBuf,
    payload: Option<Arc<Payload>>,
    found: Found,
    mode: Mode,
    cancel: Arc<AtomicBool>,
    labels: Vec<String>,
    steps: Rc<VecModel<StepRow>>,
    components: Rc<VecModel<CompRow>>,
    done_dir: Option<PathBuf>,
}

type Shared = Rc<RefCell<State>>;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("halcyon-setup"));

    if let Some(i) = args.iter().position(|a| a == "--pack") {
        std::process::exit(match pack_cli(&exe, &args[i + 1..]) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("halcyon-setup --pack : {e}");
                1
            }
        });
    }
    if args.iter().any(|a| a == "--info") {
        match Payload::from_exe(&exe) {
            Ok(Some(p)) => println!("{}", serde_json::to_string_pretty(&p.manifest).unwrap_or_default()),
            Ok(None) => println!("aucune charge utile (programme de maintenance)"),
            Err(e) => {
                eprintln!("charge utile illisible : {e}");
                std::process::exit(1);
            }
        }
        return;
    }

    let env = Env::system();
    let found = engine::detect(&env);

    // Windows : un programme ne peut pas supprimer son propre exécutable. Lancé depuis le
    // dossier d'installation, il se relance depuis une copie temporaire.
    #[cfg(windows)]
    if !args.iter().any(|a| a == "--from-temp") {
        if let Found::Installed { record, .. } = &found {
            if same_tree(&exe, &record.options.dir) {
                let tmp = std::env::temp_dir().join(format!("halcyon-setup-{}.exe", std::process::id()));
                if std::fs::copy(&exe, &tmp).is_ok() {
                    let mut a = args.clone();
                    a.push("--from-temp".into());
                    if std::process::Command::new(&tmp).args(&a).spawn().is_ok() {
                        return;
                    }
                }
            }
        }
    }

    let ui = match SetupWindow::new() {
        Ok(ui) => ui,
        Err(e) => {
            eprintln!("interface indisponible : {e}");
            std::process::exit(1);
        }
    };
    let s = ui.global::<Setup>();
    let (payload, payload_error) = match Payload::from_exe(&exe) {
        Ok(p) => (p.map(Arc::new), None),
        Err(e) => (None, Some(e)),
    };
    let version = payload.as_ref().map(|p| p.manifest.version.clone()).unwrap_or_else(|| match &found {
        Found::Installed { record, .. } => record.version.clone(),
        _ => env!("CARGO_PKG_VERSION").to_string(),
    });
    s.set_version(version.as_str().into());

    let steps = Rc::new(VecModel::<StepRow>::default());
    let components = Rc::new(VecModel::<CompRow>::default());
    s.set_steps(ModelRc::from(steps.clone()));
    s.set_components(ModelRc::from(components.clone()));

    let state: Shared = Rc::new(RefCell::new(State {
        env,
        exe,
        payload,
        found,
        mode: Mode::Install,
        cancel: Arc::new(AtomicBool::new(false)),
        labels: Vec::new(),
        steps,
        components,
        done_dir: None,
    }));

    if let Some(e) = payload_error {
        s.set_error(format!("La charge utile de ce programme d'installation est illisible ({e}). Le fichier est probablement incomplet : téléchargez-le à nouveau.").into());
        s.set_screen(9);
    } else {
        describe_found(&ui, &state.borrow());
        if state.borrow().payload.is_none() && matches!(state.borrow().found, Found::Nothing) {
            s.set_error("Halcyon n'est pas installé, et ce programme de maintenance ne contient pas ses fichiers. Lancez le programme d'installation téléchargé (halcyon-…-setup).".into());
            s.set_screen(9);
        }
    }

    wire(&ui, &state);
    if s.get_screen() == 0 && state.borrow().payload.is_none() {
        s.set_screen(1);
    }

    if s.get_screen() != 9 {
        if args.iter().any(|a| a == "--uninstall") && s.get_can_uninstall() {
            s.invoke_choose(4);
        } else if args.iter().any(|a| a == "--repair") && s.get_can_repair() {
            s.invoke_choose(3);
        }
    }

    {
        let weak = ui.as_weak();
        ui.window().on_close_requested(move || match weak.upgrade() {
            Some(ui) if ui.global::<Setup>().get_busy() => slint::CloseRequestResponse::KeepWindowShown,
            _ => slint::CloseRequestResponse::HideWindow,
        });
    }

    if let Err(e) = ui.run() {
        eprintln!("{e}");
    }

    #[cfg(windows)]
    if args.iter().any(|a| a == "--from-temp") {
        // La copie temporaire se supprime une fois fermée.
        use std::os::windows::process::CommandExt;
        let me = state.borrow().exe.clone();
        let _ = std::process::Command::new("cmd")
            .raw_arg(format!("/C ping -n 3 127.0.0.1 >NUL & del /f /q \"{}\"", me.display()))
            .creation_flags(0x0800_0000)
            .spawn();
    }
}

#[cfg(windows)]
fn same_tree(exe: &Path, dir: &Path) -> bool {
    match (exe.canonicalize(), dir.canonicalize()) {
        (Ok(e), Ok(d)) => e.starts_with(d),
        _ => false,
    }
}

// ---------------------------------------------------------------------------- existant

fn payload_version(st: &State) -> Option<String> {
    st.payload.as_ref().map(|p| p.manifest.version.clone())
}

fn describe_found(ui: &SetupWindow, st: &State) {
    let s = ui.global::<Setup>();
    let pv = payload_version(st);
    let (kind, title, detail, repair, uninstall) = match &st.found {
        Found::Nothing => (0, String::new(), String::new(), false, false),
        Found::Installed { record, health } => {
            let dir = record.options.dir.display();
            if !health.ok() {
                let n = health.missing.len() + health.corrupt.len();
                (3, format!("Halcyon {} est endommagé.", record.version), format!("Dans {dir} : {n} fichier(s) manquant(s) ou modifié(s). La réparation les rétablit sans toucher à vos données."), true, true)
            } else if pv.as_deref().is_some_and(|v| engine::compare_versions(&record.version, v) == Some(std::cmp::Ordering::Less)) {
                (2, format!("Halcyon {} est installé.", record.version), format!("Dans {dir}. La version {} est disponible.", pv.unwrap_or_default()), true, true)
            } else {
                (1, format!("Halcyon {} est installé.", record.version), format!("Dans {dir}. Tous les fichiers sont intacts."), true, true)
            }
        }
        Found::Interrupted { dir } => (
            3,
            "Une installation a été interrompue.".into(),
            format!("Dans {}. La réparation remet l'installation dans un état cohérent ; vos données ne sont pas touchées.", dir.display()),
            st.payload.is_some(),
            false,
        ),
        Found::Unmanaged { dir, version, origin } => {
            let v = version.as_deref().map(|v| format!(" {v}")).unwrap_or_default();
            let detail = match origin {
                Origin::InnoSetup => format!("Installée par l'ancien programme d'installation dans {}. La mise à jour la reprend au même endroit ; bibliothèque, paramètres et sauvegardes sont conservés.", dir.display()),
                Origin::ArchiveScript => format!(
                    "Installée par le script de l'archive ({}). La mise à jour installe Halcyon dans {} et remplace l'ancien exécutable ; bibliothèque, paramètres et sauvegardes sont conservés.",
                    platform_bin(&st.env).display(),
                    st.env.default_dir.display()
                ),
                Origin::Partial => format!("Halcyon est présent dans {} sans fiche d'installation. La mise à jour le reprend au même endroit ; vos données sont conservées.", dir.display()),
            };
            (4, format!("Une version précédente de Halcyon{v} est présente."), detail, false, false)
        }
    };
    s.set_found_kind(kind);
    s.set_found_title(title.into());
    s.set_found_detail(detail.into());
    s.set_can_repair(repair);
    s.set_can_uninstall(uninstall);
    s.set_has_payload(st.payload.is_some());
    s.set_can_browse(can_browse());
    s.set_desktop_available(st.env.desktop.is_some());
    s.set_dir(default_options(st).dir.display().to_string().into());
    let all = all_users_available();
    s.set_all_users_available(all.0);
    s.set_all_users_note(all.1.into());
    s.set_data_paths(st.env.user_data.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ").into());
}

fn platform_bin(env: &Env) -> PathBuf {
    env.bin_dir.join(platform::main_exe_name())
}

/// Options de départ : celles de l'installation existante, sinon les valeurs par défaut.
fn default_options(st: &State) -> Options {
    match &st.found {
        Found::Installed { record, .. } => record.options.clone(),
        Found::Interrupted { dir } => Options { dir: dir.clone(), menu_entry: true, desktop_shortcut: true, autostart: false, all_users: false },
        Found::Unmanaged { dir, origin, .. } => {
            let dir = if *origin == Origin::ArchiveScript { st.env.default_dir.clone() } else { dir.clone() };
            Options { dir, menu_entry: true, desktop_shortcut: true, autostart: false, all_users: false }
        }
        Found::Nothing => Options { dir: st.env.default_dir.clone(), menu_entry: true, desktop_shortcut: true, autostart: false, all_users: false },
    }
}

fn all_users_available() -> (bool, &'static str) {
    #[cfg(windows)]
    {
        if platform::windows::elevated() {
            (true, "Installe dans Program Files, avec les raccourcis communs.")
        } else {
            (false, "Lancez le programme d'installation en tant qu'administrateur pour installer pour tous.")
        }
    }
    #[cfg(not(windows))]
    {
        (false, "Sous Linux, l'installation système passe par le paquet .deb.")
    }
}

fn mode_noun(m: Mode) -> &'static str {
    match m {
        Mode::Install => "Installation",
        Mode::Update => "Mise à jour",
        Mode::Reinstall => "Réinstallation",
        Mode::Repair => "Réparation",
    }
}

fn mode_label(m: Mode) -> &'static str {
    match m {
        Mode::Install => "Installer",
        Mode::Update => "Mettre à jour",
        Mode::Reinstall => "Réinstaller",
        Mode::Repair => "Réparer",
    }
}

// ------------------------------------------------------------------------------- liaisons

fn ui_options(s: &Setup) -> Options {
    Options {
        dir: PathBuf::from(s.get_dir().trim()),
        menu_entry: s.get_menu(),
        desktop_shortcut: s.get_desktop(),
        autostart: s.get_autostart(),
        all_users: s.get_all_users(),
    }
}

fn apply_options(s: &Setup, o: &Options) {
    s.set_dir(o.dir.display().to_string().into());
    s.set_menu(o.menu_entry);
    s.set_desktop(o.desktop_shortcut && s.get_desktop_available());
    s.set_autostart(o.autostart);
    s.set_all_users(o.all_users);
}

fn refresh_space(s: &Setup, st: &State) {
    let total = st.payload.as_ref().map(|p| p.manifest.total_size()).unwrap_or(0);
    s.set_required(engine::fmt_size(engine::required_space(total)).into());
    let dir = PathBuf::from(s.get_dir().trim());
    s.set_free(if dir.is_absolute() { engine::free_space(&dir).map(engine::fmt_size).unwrap_or_default() } else { String::new() }.into());
}

fn wire(ui: &SetupWindow, state: &Shared) {
    let s = ui.global::<Setup>();

    {
        let (w, st) = (ui.as_weak(), state.clone());
        s.on_start(move || {
            let Some(ui) = w.upgrade() else { return };
            let s = ui.global::<Setup>();
            let mut stt = st.borrow_mut();
            if matches!(stt.found, Found::Nothing) {
                stt.mode = Mode::Install;
                let o = default_options(&stt);
                apply_options(&s, &o);
                s.set_mode_label(mode_label(Mode::Install).into());
                s.set_mode_noun(mode_noun(Mode::Install).into());
                s.set_dir_locked(false);
                s.set_skip_config(false);
                refresh_space(&s, &stt);
                s.set_screen(2);
            } else {
                s.set_screen(1);
            }
        });
    }
    {
        let (w, st) = (ui.as_weak(), state.clone());
        s.on_choose(move |i| {
            let Some(ui) = w.upgrade() else { return };
            let s = ui.global::<Setup>();
            if i == 4 {
                s.set_remove_cache(false);
                s.set_remove_data(false);
                s.set_confirm_data(false);
                s.set_error("".into());
                s.set_busy(false);
                s.set_screen(7);
                return;
            }
            let mode = match i {
                1 => Mode::Update,
                2 => Mode::Reinstall,
                _ => Mode::Repair,
            };
            let (no_payload, locked) = {
                let mut stt = st.borrow_mut();
                stt.mode = mode;
                let o = default_options(&stt);
                apply_options(&s, &o);
                refresh_space(&s, &stt);
                // Une installation existante reste à sa place (sinon l'ancienne resterait).
                let locked = !matches!(&stt.found, Found::Unmanaged { origin: Origin::ArchiveScript, .. });
                (stt.payload.is_none(), locked)
            };
            s.set_mode_label(mode_label(mode).into());
            s.set_mode_noun(mode_noun(mode).into());
            s.set_dir_locked(locked);
            s.set_skip_config(mode == Mode::Repair);
            if mode == Mode::Repair {
                if no_payload {
                    s.invoke_install();
                } else {
                    s.invoke_check();
                }
            } else {
                s.set_screen(2);
            }
        });
    }
    {
        let (w, st) = (ui.as_weak(), state.clone());
        s.on_dir_changed(move |_| {
            let Some(ui) = w.upgrade() else { return };
            let s = ui.global::<Setup>();
            #[cfg(windows)]
            {
                // Pour tous : Program Files ; pour soi : dossier personnel.
                let user = st.borrow().env.default_dir.clone();
                let pf = std::env::var_os("ProgramFiles").map(|p| PathBuf::from(p).join("Halcyon"));
                let cur = PathBuf::from(s.get_dir().trim());
                if let Some(pf) = pf {
                    if s.get_all_users() && cur == user {
                        s.set_dir(pf.display().to_string().into());
                    } else if !s.get_all_users() && cur == pf {
                        s.set_dir(user.display().to_string().into());
                    }
                }
            }
            refresh_space(&s, &st.borrow());
        });
    }
    {
        let w = ui.as_weak();
        s.on_browse(move || {
            let Some(ui) = w.upgrade() else { return };
            if let Some(p) = browse() {
                let s = ui.global::<Setup>();
                let t: slint::SharedString = p.display().to_string().into();
                s.set_dir(t.clone());
                s.invoke_dir_changed(t);
            }
        });
    }
    {
        let (w, st) = (ui.as_weak(), state.clone());
        s.on_check(move || {
            let Some(ui) = w.upgrade() else { return };
            let s = ui.global::<Setup>();
            let stt = st.borrow();
            let Some(p) = stt.payload.clone() else { return };
            let opts = ui_options(&s);
            let (found, mode) = (stt.found.clone(), stt.mode);
            s.set_checks(ModelRc::new(VecModel::<CheckRow>::default()));
            s.set_checking(true);
            s.set_checks_ok(false);
            s.set_screen(3);
            let w = ui.as_weak();
            std::thread::spawn(move || {
                let checks = checks_for(&p, &opts, &found, mode);
                let _ = slint::invoke_from_event_loop(move || {
                    let Some(ui) = w.upgrade() else { return };
                    let s = ui.global::<Setup>();
                    s.set_checks_ok(!checks.iter().any(|c| c.level == Level::Error));
                    let rows: Vec<CheckRow> = checks
                        .into_iter()
                        .map(|c| CheckRow {
                            label: c.label.into(),
                            detail: c.detail.into(),
                            level: match c.level {
                                Level::Ok => 0,
                                Level::Info => 1,
                                Level::Warning => 2,
                                Level::Error => 3,
                            },
                        })
                        .collect();
                    s.set_checks(ModelRc::new(VecModel::from(rows)));
                    s.set_checking(false);
                });
            });
        });
    }
    {
        let (w, st) = (ui.as_weak(), state.clone());
        s.on_install(move || {
            let Some(ui) = w.upgrade() else { return };
            start_install(&ui, &st);
        });
    }
    {
        let st = state.clone();
        s.on_cancel(move || st.borrow().cancel.store(true, Ordering::SeqCst));
    }
    {
        let (w, st) = (ui.as_weak(), state.clone());
        s.on_uninstall(move || {
            let Some(ui) = w.upgrade() else { return };
            start_uninstall(&ui, &st);
        });
    }
    {
        let (w, st) = (ui.as_weak(), state.clone());
        s.on_launch(move || {
            let Some(ui) = w.upgrade() else { return };
            let Some(dir) = st.borrow().done_dir.clone() else { return };
            match launch(&dir) {
                // Le travail est fini : l'installateur se ferme dès que Halcyon démarre.
                Ok(()) => {
                    let _ = slint::quit_event_loop();
                }
                Err(e) => ui.global::<Setup>().set_launch_error(format!("Halcyon n'a pas pu être lancé : {e}").into()),
            }
        });
    }
    {
        let st = state.clone();
        s.on_open_docs(move || {
            if let Some(dir) = st.borrow().done_dir.clone() {
                open_path(&dir.join("GUIDE.md"));
            }
        });
    }
    {
        let w = ui.as_weak();
        s.on_close(move || {
            if let Some(ui) = w.upgrade() {
                if ui.global::<Setup>().get_busy() {
                    return;
                }
            }
            let _ = slint::quit_event_loop();
        });
    }
    {
        let w = ui.as_weak();
        s.on_minimize(move || {
            if let Some(ui) = w.upgrade() {
                ui.window().set_minimized(true);
            }
        });
    }
    {
        let w = ui.as_weak();
        s.on_drag(move || {
            use slint::winit_030::WinitWindowAccessor;
            if let Some(ui) = w.upgrade() {
                ui.window().with_winit_window(|win| {
                    let _ = win.drag_window();
                });
            }
        });
    }
}

/// Vérifications du moteur, complétées par celles du dossier choisi.
fn checks_for(p: &Payload, opts: &Options, found: &Found, mode: Mode) -> Vec<engine::Check> {
    let mut out = Vec::new();
    if !opts.dir.is_absolute() {
        out.push(engine::Check { label: "Dossier".into(), detail: "indiquez un chemin complet (par exemple celui proposé par défaut)".into(), level: Level::Error });
        return out;
    }
    let ours = opts.dir.join(engine::RECORD).is_file() || opts.dir.join(platform::main_exe_name()).is_file();
    if !ours && std::fs::read_dir(&opts.dir).is_ok_and(|mut d| d.next().is_some()) {
        out.push(engine::Check {
            label: "Dossier".into(),
            detail: "il contient déjà des fichiers : ils ne seront ni modifiés ni supprimés, y compris à la désinstallation".into(),
            level: Level::Warning,
        });
    }
    out.extend(engine::system_checks(p, opts, found, mode));
    out
}

// --------------------------------------------------------------------------- progression

/// Envoie les évènements du moteur à l'interface (au plus ~25 mises à jour par seconde pour
/// la progression ; étapes, composants et avertissements toujours transmis).
struct Forward {
    w: slint::Weak<SetupWindow>,
    last: Instant,
}

impl Forward {
    fn send(&mut self, ev: Event) {
        let frequent = matches!(ev, Event::Bytes(d, t) if d < t);
        if frequent && self.last.elapsed() < Duration::from_millis(40) {
            return;
        }
        self.last = Instant::now();
        let w = self.w.clone();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = w.upgrade() {
                STATE.with(|cell| {
                    if let Some(st) = cell.borrow().as_ref() {
                        apply_event(&ui, &st.borrow(), ev);
                    }
                });
            }
        });
    }
}

thread_local! {
    static STATE: RefCell<Option<Shared>> = const { RefCell::new(None) };
}

fn set_step(st: &State, active: usize, failed: bool) {
    for i in 0..st.steps.row_count() {
        let state = if i < active { 2 } else if i == active { if failed { 3 } else { 1 } } else { 0 };
        st.steps.set_row_data(i, StepRow { label: st.labels[i].as_str().into(), state });
    }
}

fn current_step(st: &State) -> usize {
    (0..st.steps.row_count()).find(|&i| st.steps.row_data(i).is_some_and(|r| r.state == 1 || r.state == 3)).unwrap_or(0)
}

fn apply_event(ui: &SetupWindow, st: &State, ev: Event) {
    let s = ui.global::<Setup>();
    match ev {
        Event::Step(i) => {
            set_step(st, i, false);
            s.set_progress(-1.0);
            s.set_progress_text(st.labels.get(i).cloned().unwrap_or_default().into());
            s.set_current_file("".into());
            if s.get_screen() == 4 {
                // L'annulation n'est possible que tant que rien n'est en place.
                s.set_cancellable(i <= 1 && st.labels.len() == engine::STEPS.len());
                if i >= st.labels.len() - 2 {
                    s.set_screen(5);
                }
            }
        }
        Event::Bytes(done, total) => {
            let label = st.labels.get(current_step(st)).cloned().unwrap_or_default();
            if total > 0 {
                s.set_progress((done as f64 / total as f64).clamp(0.0, 1.0) as f32);
                s.set_progress_text(format!("{label} · {} sur {}", engine::fmt_size(done), engine::fmt_size(total)).into());
            }
        }
        Event::File(f) => s.set_current_file(f.into()),
        Event::Component(label, ok, detail) => st.components.push(CompRow { label: label.into(), ok, detail: detail.into() }),
        Event::Warning(w) => st.components.push(CompRow { label: "Avertissement".into(), ok: false, detail: w.into() }),
    }
}

fn begin(ui: &SetupWindow, st: &mut State, labels: Vec<String>) {
    let s = ui.global::<Setup>();
    st.labels = labels;
    st.steps.set_vec(st.labels.iter().map(|l| StepRow { label: l.as_str().into(), state: 0 }).collect::<Vec<_>>());
    st.components.set_vec(Vec::new());
    st.cancel.store(false, Ordering::SeqCst);
    s.set_error("".into());
    s.set_warnings("".into());
    s.set_launch_error("".into());
    s.set_progress(-1.0);
    s.set_progress_text("Démarrage…".into());
    s.set_current_file("".into());
    s.set_busy(true);
}

fn start_install(ui: &SetupWindow, state: &Shared) {
    let s = ui.global::<Setup>();
    let opts = ui_options(&s);
    let (payload, env, mode, previous, exe, cancel) = {
        let mut st = state.borrow_mut();
        let labels: &[&str] = if st.payload.is_some() { &engine::STEPS } else { &engine::REPAIR_STEPS };
        begin(ui, &mut st, labels.iter().map(|l| l.to_string()).collect());
        let previous = match &st.found {
            Found::Installed { record, .. } => Some(record.clone()),
            _ => None,
        };
        (st.payload.clone(), st.env.clone(), st.mode, previous, st.exe.clone(), st.cancel.clone())
    };
    s.set_cancellable(payload.is_some());
    s.set_screen(4);
    STATE.with(|c| *c.borrow_mut() = Some(state.clone()));
    let w = ui.as_weak();
    std::thread::spawn(move || {
        let mut fwd = Forward { w: w.clone(), last: Instant::now() - Duration::from_secs(1) };
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match (&payload, &previous) {
            (Some(p), _) => engine::run(p, &opts, &env, mode, previous.as_ref(), Some(&exe), &cancel, &mut |e| fwd.send(e)),
            (None, Some(rec)) => engine::repair_integration(rec, &env, &mut |e| fwd.send(e)),
            (None, None) => Err("aucun fichier à installer".into()),
        }))
        .unwrap_or_else(|_| Err("erreur interne du programme d'installation".into()));
        let dir = opts.dir.clone();
        let version = engine::read_record(&dir).map(|r| r.version).unwrap_or_default();
        let _ = slint::invoke_from_event_loop(move || {
            let Some(ui) = w.upgrade() else { return };
            let s = ui.global::<Setup>();
            let Some(state) = STATE.with(|c| c.borrow().clone()) else { return };
            let mut st = state.borrow_mut();
            s.set_busy(false);
            s.set_cancellable(false);
            match result {
                Ok(report) => {
                    let n = st.steps.row_count();
                    set_step(&st, n, false);
                    s.set_progress(1.0);
                    s.set_progress_text("Terminé".into());
                    let (title, text) = match mode {
                        Mode::Install => ("Halcyon est prêt.".to_string(), "Installation terminée. Au premier lancement, Halcyon détecte les jeux déjà installés (Steam, Epic Games, standalone).".to_string()),
                        Mode::Update => (format!("Halcyon {version} est prêt."), "Mise à jour terminée. Votre bibliothèque, vos paramètres et vos sauvegardes sont conservés.".to_string()),
                        Mode::Reinstall => ("Halcyon est réinstallé.".to_string(), "Tous les fichiers du programme ont été réécrits. Vos données sont intactes.".to_string()),
                        Mode::Repair => (
                            "Réparation terminée.".to_string(),
                            if report.written > 0 { format!("{} fichier(s) rétabli(s) ; raccourcis et entrées du système recréés.", report.written) } else { "Les fichiers étaient intacts ; raccourcis et entrées du système recréés.".to_string() },
                        ),
                    };
                    s.set_done_title(title.into());
                    s.set_done_text(text.into());
                    s.set_warnings(if report.warnings.is_empty() { String::new() } else { format!("À noter : {}", report.warnings.join(" ; ")) }.into());
                    s.set_has_docs(dir.join("GUIDE.md").is_file());
                    st.done_dir = Some(dir);
                    st.found = engine::detect(&st.env);
                    describe_found(&ui, &st);
                    s.set_screen(6);
                }
                Err(e) => {
                    let cur = current_step(&st);
                    set_step(&st, cur, true);
                    s.set_error(capitalize(&e).into());
                    s.set_progress(0.0);
                    st.found = engine::detect(&st.env);
                    describe_found(&ui, &st);
                }
            }
        });
    });
}

fn start_uninstall(ui: &SetupWindow, state: &Shared) {
    let s = ui.global::<Setup>();
    let opts = UninstallOptions { remove_cache: s.get_remove_cache(), remove_data: s.get_remove_data() && s.get_confirm_data() };
    let (record, env): (Record, Env) = {
        let mut st = state.borrow_mut();
        let Found::Installed { record, .. } = &st.found else { return };
        let record = record.clone();
        begin(ui, &mut st, UNINSTALL_STEPS.iter().map(|l| l.to_string()).collect());
        (record, st.env.clone())
    };
    s.set_cancellable(false);
    STATE.with(|c| *c.borrow_mut() = Some(state.clone()));
    let w = ui.as_weak();
    std::thread::spawn(move || {
        let mut fwd = Forward { w: w.clone(), last: Instant::now() - Duration::from_secs(1) };
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| engine::uninstall(&record, &env, &opts, &mut |e| fwd.send(e))))
            .unwrap_or_else(|_| Err("erreur interne du programme de désinstallation".into()));
        let _ = slint::invoke_from_event_loop(move || {
            let Some(ui) = w.upgrade() else { return };
            let s = ui.global::<Setup>();
            let Some(state) = STATE.with(|c| c.borrow().clone()) else { return };
            let mut st = state.borrow_mut();
            s.set_busy(false);
            match result {
                Ok(report) => {
                    let n = st.steps.row_count();
                    set_step(&st, n, false);
                    s.set_done_title("Halcyon a été désinstallé.".into());
                    s.set_done_text(
                        if opts.remove_data { "Le programme et vos données Halcyon ont été supprimés. Vos jeux n'ont pas été touchés." } else { "Le programme a été retiré. Vos données Halcyon sont conservées : une réinstallation les retrouvera. Vos jeux n'ont pas été touchés." }
                            .into(),
                    );
                    s.set_warnings(if report.warnings.is_empty() { String::new() } else { format!("À noter : {}", report.warnings.join(" ; ")) }.into());
                    st.found = engine::detect(&st.env);
                    describe_found(&ui, &st);
                    s.set_screen(8);
                }
                Err(e) => {
                    let cur = current_step(&st);
                    set_step(&st, cur, true);
                    s.set_error(capitalize(&e).into());
                    s.set_progress(0.0);
                }
            }
        });
    });
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

// ------------------------------------------------------------------------------- système

fn launch(dir: &Path) -> Result<(), String> {
    let exe = dir.join(platform::main_exe_name());
    let mut cmd = std::process::Command::new(&exe);
    cmd.current_dir(dir).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    #[cfg(unix)]
    {
        // Halcyon ne dépend pas de l'installateur : il continue après sa fermeture.
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    cmd.spawn().map(|_| ()).map_err(|e| format!("{} : {e}", exe.display()))
}

fn open_path(p: &Path) {
    #[cfg(windows)]
    let r = {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("cmd").raw_arg(format!("/C start \"\" \"{}\"", p.display())).creation_flags(0x0800_0000).spawn()
    };
    #[cfg(not(windows))]
    let r = std::process::Command::new("xdg-open").arg(p).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn();
    if let Err(e) = r {
        eprintln!("ouverture de {} : {e}", p.display());
    }
}

#[cfg(not(windows))]
fn in_path(tool: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(tool).is_file()))
}

fn can_browse() -> bool {
    #[cfg(windows)]
    {
        true
    }
    #[cfg(not(windows))]
    {
        in_path("zenity") || in_path("kdialog")
    }
}

/// Sélecteur de dossier du système (zenity ou kdialog sous Linux, PowerShell sous Windows).
/// Un sous-dossier « Halcyon » est ajouté, comme le font les installateurs habituels.
fn browse() -> Option<PathBuf> {
    #[cfg(windows)]
    let out = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-STA",
            "-Command",
            "[Console]::OutputEncoding = [Text.Encoding]::UTF8; Add-Type -AssemblyName System.Windows.Forms; $d = New-Object System.Windows.Forms.FolderBrowserDialog; \
             $d.Description = 'Dossier d''installation de Halcyon'; if ($d.ShowDialog() -eq 'OK') { [Console]::Out.Write($d.SelectedPath) }",
        ])
        .output()
        .ok()?;
    #[cfg(not(windows))]
    let out = if in_path("zenity") {
        std::process::Command::new("zenity").args(["--file-selection", "--directory", "--title=Dossier d'installation de Halcyon"]).output().ok()?
    } else {
        std::process::Command::new("kdialog").args(["--getexistingdirectory", "--title", "Dossier d'installation de Halcyon"]).output().ok()?
    };
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if !out.status.success() || text.is_empty() {
        return None;
    }
    let p = PathBuf::from(text);
    let name = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    Some(if name == "halcyon" { p } else { p.join(if cfg!(windows) { "Halcyon" } else { "halcyon" }) })
}

// ---------------------------------------------------------------------------------- --pack

fn pack_cli(stub: &Path, args: &[String]) -> Result<(), String> {
    let mut pos = Vec::new();
    let mut min_glibc = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--min-glibc" {
            min_glibc = it.next().cloned();
        } else {
            pos.push(a.clone());
        }
    }
    let [dir, version, out] = <[String; 3]>::try_from(pos).map_err(|_| "usage : --pack <dossier> <version> <sortie> [--min-glibc X.Y]".to_string())?;
    let dir = PathBuf::from(dir);
    if !dir.join(platform::main_exe_name()).is_file() {
        return Err(format!("{} absent de {}", platform::main_exe_name(), dir.display()));
    }
    let manifest = payload::Manifest {
        product: "Halcyon".into(),
        version,
        platform: platform::current_platform(),
        main: platform::main_exe_name().into(),
        min_glibc,
        files: Vec::new(),
    };
    let m = payload::pack(stub, &dir, manifest, Path::new(&out))?;
    println!("{out} : Halcyon {} ({}), {} fichier(s), {}", m.version, m.platform, m.files.len(), engine::fmt_size(m.total_size()));
    // Le paquet produit doit se relire.
    match Payload::from_exe(Path::new(&out))? {
        Some(p) if p.manifest == m => Ok(()),
        _ => Err("la charge utile produite ne se relit pas".into()),
    }
}


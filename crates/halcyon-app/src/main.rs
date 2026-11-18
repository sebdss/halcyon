//! Halcyon — launcher universel de jeux vidéo.

// Pas de console noire derrière la fenêtre en version publiée sous Windows.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[macro_use]
extern crate halcyon_i18n;

mod controller;
mod ctl_achievements;
mod ctl_assistant;
mod ctl_backup;
mod ctl_bigpicture;
mod ctl_downloads;
mod ctl_emulators;
mod ctl_home;
mod ctl_lang;
mod ctl_library;
mod ctl_palette;
mod ctl_minecraft;
mod ctl_mods;
mod ctl_pads;
mod ctl_perf;
mod ctl_plugins;
mod ctl_profiles;
mod ctl_recap;
mod ctl_remote;
mod ctl_saves;
mod ctl_shell;
mod ctl_sound;
mod ctl_system;
mod ctl_themes;
mod ctl_tracking;
mod ctl_update;
mod desktop;
mod dialogs;
mod gamepad;
mod images;
mod instance;
mod lang;
mod models;
mod nav;
mod notify;
mod perf;
mod remote;
mod tasks;
mod tray;

use halcyon_storage::{logging, AppPaths, Database, Settings};
use slint::ComponentHandle;
use std::time::{Duration, Instant};

slint::include_modules!();

fn main() {
    let started = Instant::now();
    let paths = AppPaths::resolve();
    if let Err(e) = paths.ensure() {
        eprintln!("Impossible de créer les dossiers de données ({}) : {e}", paths.data_dir.display());
    }
    // Instance unique : la demande est transmise à Halcyon déjà ouvert.
    let args = Args::parse();
    if instance::forward(&paths.data_dir, &args.command()) {
        return;
    }
    let level = if std::env::var_os("HALCYON_DEBUG").is_some() { log::LevelFilter::Debug } else { log::LevelFilter::Info };
    logging::init(&paths.logs_dir, level);
    halcyon_storage::audit::init(&paths.logs_dir);
    log::info!("Halcyon {} — données : {}", env!("CARGO_PKG_VERSION"), paths.data_dir.display());

    let first_run = !paths.settings_path().exists();
    let mut settings = Settings::load(&paths.settings_path());
    lang::init(&mut settings, first_run);
    if first_run && settings.language.is_some() {
        let _ = settings.save(&paths.settings_path());
    }
    // Mise à jour : reprise d'une installation interrompue, ou installation de la version en
    // attente ; l'updater prend alors le relais et relance Halcyon.
    if ctl_update::early_start(&settings, args.hus_health.as_deref(), args.hus_rolled_back) {
        instance::release(&paths.data_dir);
        log::logger().flush();
        return;
    }
    let db = open_database(&paths);
    let t_db = Instant::now();
    let games = db.load_games().unwrap_or_else(|e| {
        log::error!("lecture de la bibliothèque impossible : {e}");
        Vec::new()
    });
    log::info!("{} jeux chargés depuis la base ({} ms)", games.len(), t_db.elapsed().as_millis());

    let t_ui = Instant::now();
    let app = match AppWindow::new() {
        Ok(app) => app,
        Err(e) => {
            log::error!("création de la fenêtre impossible : {e}");
            log::logger().flush();
            std::process::exit(1);
        }
    };
    lang::apply_to_ui();
    app.window().set_size(slint::LogicalSize::new(settings.window_width as f32, settings.window_height as f32));
    if settings.window_maximized {
        app.window().set_maximized(true);
    }

    log::debug!("création de l'interface : {} ms", t_ui.elapsed().as_millis());
    let data_dir = paths.data_dir.clone();
    let ctrl = controller::Controller::install(&app, paths, db, settings, games);
    instance::listen(&data_dir, |cmd| controller::post(move |c| c.on_instance_command(cmd)));
    log::info!("interface prête en {} ms", started.elapsed().as_millis());

    ctl_bigpicture::prefetch_clock();
    gamepad::spawn(|ev| controller::post(move |c| c.on_pad_event(ev)));
    // Laisse la première image s'afficher avant les tâches de fond.
    let weak = std::rc::Rc::downgrade(&ctrl);
    slint::Timer::single_shot(Duration::from_millis(400), move || {
        if let Some(c) = weak.upgrade() {
            c.startup();
            c.hus_init(args.hus_health.clone());
            // `--minimized` (démarrage avec la session) : dans la zone de notification si possible.
            if args.minimized {
                if c.tray.borrow().is_some() {
                    if let Some(app) = c.app() {
                        let _ = app.hide();
                    }
                } else if let Some(app) = c.app() {
                    app.window().set_minimized(true);
                }
            }
            match args.command() {
                // `--launch <id>` : lancement direct (raccourcis, Sunshine), avec profil, mods et sauvegardes.
                instance::Command::Launch(id) => c.play(id),
                instance::Command::Open(p) => c.open_path(&p),
                instance::Command::Quick => c.open_quick_launch(),
                instance::Command::BigPicture => c.enter_big_picture(),
                instance::Command::Show => {}
            }
        }
    });

    // La boucle continue fenêtre masquée (zone de notification) : `quit_event_loop` l'arrête.
    if let Err(e) = app.show().and_then(|_| slint::run_event_loop_until_quit()) {
        log::error!("boucle d'événements interrompue : {e}");
    }
    instance::release(&data_dir);
    log::info!("fermeture");
    log::logger().flush();
}

/// Ouvre la base ; si elle est illisible, elle est mise de côté et une base neuve est créée
/// (la bibliothèque se reconstruit à la prochaine analyse).
fn open_database(paths: &AppPaths) -> Database {
    let path = paths.db_path();
    match Database::open(&path) {
        Ok(db) => db,
        Err(e) => {
            log::error!("base de données illisible ({e}) : sauvegarde et recréation");
            let backup = path.with_extension(format!("corrupt-{}", halcyon_storage::now_unix()));
            let _ = std::fs::rename(&path, &backup);
            Database::open(&path).unwrap_or_else(|e| {
                log::error!("recréation impossible ({e}) : base en mémoire pour cette session");
                Database::open_in_memory().expect("SQLite en mémoire")
            })
        }
    }
}

/// Arguments de la ligne de commande.
#[derive(Default)]
struct Args {
    launch: Option<halcyon_core::model::GameId>,
    open: Option<std::path::PathBuf>,
    quick: bool,
    big_picture: bool,
    minimized: bool,
    /// Contrôle de santé demandé par l'updater (identifiant de transaction).
    hus_health: Option<String>,
    /// Relancé par l'updater après un retour arrière.
    hus_rolled_back: bool,
}

impl Args {
    fn parse() -> Self {
        let mut a = Args::default();
        let mut it = std::env::args().skip(1);
        while let Some(arg) = it.next() {
            match arg.as_str() {
                "--launch" => a.launch = it.next().and_then(|v| v.parse().ok()),
                "--quick" => a.quick = true,
                "--big-picture" => a.big_picture = true,
                "--minimized" => a.minimized = true,
                "--hus-health" => a.hus_health = it.next(),
                "--hus-rolled-back" => a.hus_rolled_back = true,
                other if !other.starts_with("--") && std::path::Path::new(other).is_file() => {
                    a.open = Some(std::path::PathBuf::from(other));
                }
                _ => {}
            }
        }
        a
    }

    fn command(&self) -> instance::Command {
        if let Some(id) = self.launch {
            instance::Command::Launch(id)
        } else if let Some(p) = &self.open {
            instance::Command::Open(p.clone())
        } else if self.quick {
            instance::Command::Quick
        } else if self.big_picture {
            instance::Command::BigPicture
        } else {
            instance::Command::Show
        }
    }
}

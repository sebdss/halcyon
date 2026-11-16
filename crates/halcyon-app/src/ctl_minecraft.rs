//! Contrôleur — onglet Minecraft : versions, loaders, mods, modpacks, compte, lancement.
//!
//! Toutes les opérations longues tournent sur un fil de travail et rapportent une
//! progression réelle (octets et fichiers) ; l'interface n'est jamais bloquée.

use crate::controller::{post, Controller};
use crate::{McModRow, McProfileRow, McProjectRow, McUi, McVersionRow};
use halcyon_minecraft::auth::{self, Account, Auth};
use halcyon_minecraft::http::{Error, Http};
use halcyon_minecraft::install::{self, Event, Layout};
use halcyon_minecraft::launch::{self, Identity, Options};
use halcyon_minecraft::loaders::{self, LoaderVersion};
use halcyon_minecraft::meta::Manifest;
use halcyon_minecraft::mods::{self, Kind, Project, ProjectVersion, Source};
use halcyon_minecraft::profiles::{self, ModpackRef, Profile};
use halcyon_minecraft::{java, modpack, official, Loader};
use slint::{ComponentHandle, Image, ModelRc, SharedString, VecModel};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Default)]
pub(crate) struct McState {
    pub manifest: Option<Arc<Manifest>>,
    pub profiles: Vec<Profile>,
    pub results: Vec<Project>,
    pub packs: Vec<Project>,
    pub releases: Vec<String>,
    pub loader_versions: Vec<LoaderVersion>,
    pub updates: Vec<(String, ProjectVersion)>,
    pub account: Option<Account>,
    pub cancel: Arc<AtomicBool>,
    pub busy: bool,
    pub playing: bool,
    pub started: bool,
}

const LOADER_ORDER: [Loader; 4] = [Loader::Fabric, Loader::Quilt, Loader::Forge, Loader::NeoForge];

fn msg(e: &Error) -> String {
    let t = auth::explain(e);
    let mut c = t.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// Résultat d'une opération de fond, appliqué ensuite sur le fil de l'interface.
type Done = Box<dyn FnOnce(&Rc<Controller>) + Send>;

impl Controller {
    pub(crate) fn mc_layout(&self) -> Layout {
        Layout::new(self.paths.data_dir.join("minecraft"))
    }

    /// Première ouverture de l'onglet : état local, puis liste des versions en ligne.
    pub(crate) fn mc_open(&self) {
        let first = !std::mem::replace(&mut self.mc.borrow_mut().started, true);
        let layout = self.mc_layout();
        let _ = std::fs::create_dir_all(layout.profiles());
        if self.mc.borrow().manifest.is_none() {
            if let Ok(m) = install::manifest(&Http::new(), &layout, false) {
                self.mc.borrow_mut().manifest = Some(Arc::new(m));
            }
        }
        self.mc_reload_profiles();
        self.mc_refresh_ui();
        if first {
            self.mc_restore_account();
            self.mc_fetch_manifest();
        }
    }

    fn mc_fetch_manifest(&self) {
        if !self.net_allowed() {
            return;
        }
        let layout = self.mc_layout();
        std::thread::spawn(move || {
            if let Ok(m) = install::manifest(&Http::new(), &layout, true) {
                post(move |c| {
                    c.mc.borrow_mut().manifest = Some(Arc::new(m));
                    c.mc_refresh_versions();
                    c.mc_loader_changed();
                });
            }
        });
    }

    fn mc_reload_profiles(&self) {
        let list = profiles::list(&self.mc_layout());
        self.mc.borrow_mut().profiles = list;
    }

    fn mc_selected(&self) -> Option<Profile> {
        let app = self.app()?;
        let i = app.global::<McUi>().get_selected_profile().max(0) as usize;
        self.mc.borrow().profiles.get(i).cloned()
    }

    pub(crate) fn mc_refresh_ui(&self) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<McUi>();
        let s = self.settings.borrow().clone();
        let m = &s.minecraft;
        ui.set_play_mode(if m.play_mode == "direct" { 1 } else { 0 });
        let off = official::dir();
        ui.set_official_found(official::launcher().is_some() && off.is_some());
        ui.set_official_account(off.as_deref().and_then(official::active_account).map(|a| a.0).unwrap_or_default().into());
        ui.set_official_consent(m.official_consent);
        ui.set_client_id_set(!m.client_id.trim().is_empty());
        ui.set_client_id(m.client_id.as_str().into());
        ui.set_account_name(self.mc.borrow().account.as_ref().map(|a| a.name.clone()).unwrap_or_default().into());
        ui.set_curseforge_key(s.curseforge_api_key.as_str().into());
        ui.set_curseforge_ready(!s.curseforge_api_key.trim().is_empty());
        ui.set_ram_min(m.ram_min_mb as i32);
        ui.set_ram_max(m.ram_max_mb as i32);
        ui.set_java_path(m.java_path.as_str().into());
        ui.set_jvm_args(m.jvm_args.as_str().into());
        ui.set_width(if m.width > 0 { m.width.to_string() } else { String::new() }.into());
        ui.set_height(if m.height > 0 { m.height.to_string() } else { String::new() }.into());
        ui.set_fullscreen(m.fullscreen);
        ui.set_show_snapshots(m.show_snapshots);
        ui.set_show_old(m.show_old);
        ui.set_root_dir(self.mc_layout().root.display().to_string().into());
        let total = {
            match ::sysinfo::System::new() {
                Ok(mut sys) => {
                    sys.refresh_memory();
                    (sys.total_memory() / (1024 * 1024)) as i32
                }
                Err(_) => 0,
            }
        };
        ui.set_ram_total(total);
        // Installations
        let st = self.mc.borrow();
        let rows: Vec<McProfileRow> = st
            .profiles
            .iter()
            .map(|p| {
                let mut detail = format!("{} mod{}", p.mods.len(), if p.mods.len() > 1 { "s" } else { "" });
                if p.played_secs > 0 {
                    detail.push_str(&format!(" · {} h {:02} min de jeu", p.played_secs / 3600, p.played_secs % 3600 / 60));
                }
                McProfileRow {
                    id: p.id.as_str().into(),
                    name: p.name.as_str().into(),
                    version: format!("Minecraft {}", p.mc).into(),
                    loader: p.display_loader().into(),
                    detail: detail.into(),
                    modpack: p.modpack.is_some(),
                }
            })
            .collect();
        let labels: Vec<SharedString> = st.profiles.iter().map(|p| SharedString::from(format!("{} ({})", p.name, p.display_loader()))).collect();
        let mut sel = ui.get_selected_profile();
        if let Some(i) = st.profiles.iter().position(|p| p.id == m.last_profile) {
            if !st.started || sel < 0 {
                sel = i as i32;
            }
        }
        if sel as usize >= rows.len() {
            sel = 0;
        }
        ui.set_profiles(ModelRc::new(VecModel::from(rows)));
        ui.set_profile_labels(ModelRc::new(VecModel::from(labels)));
        ui.set_selected_profile(sel.max(0));
        ui.set_playing(st.playing);
        ui.set_busy(st.busy);
        let worlds = st.profiles.get(sel.max(0) as usize).map(|p| profiles::worlds(&p.game_dir(&self.mc_layout())).join(", ")).unwrap_or_default();
        ui.set_worlds(worlds.into());
        drop(st);
        self.mc_refresh_versions();
        self.mc_refresh_mods();
    }

    fn mc_refresh_versions(&self) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<McUi>();
        let st = self.mc.borrow();
        let Some(m) = st.manifest.clone() else {
            ui.set_latest("Liste des versions indisponible hors ligne (jamais téléchargée).".into());
            return;
        };
        drop(st);
        let installed = install::installed(&self.mc_layout());
        let s = self.settings.borrow();
        let filter = ui.get_version_filter().trim().to_lowercase();
        let rows: Vec<McVersionRow> = m
            .versions
            .iter()
            .filter(|v| match v.kind.as_str() {
                "release" => true,
                "snapshot" => s.minecraft.show_snapshots,
                _ => s.minecraft.show_old,
            })
            .filter(|v| filter.is_empty() || v.id.to_lowercase().contains(&filter))
            .take(300)
            .map(|v| McVersionRow {
                id: v.id.as_str().into(),
                kind: match v.kind.as_str() {
                    "release" => "Release",
                    "snapshot" => "Snapshot",
                    "old_beta" => "Bêta",
                    _ => "Alpha",
                }
                .into(),
                date: v.release_time.get(..10).map(|d| format!("{}/{}/{}", &d[8..10], &d[5..7], &d[..4])).unwrap_or_default().into(),
                installed: installed.contains(&v.id),
            })
            .collect();
        ui.set_latest(format!("Dernière version : {} · dernier snapshot : {} · {} versions publiées par Mojang", m.latest.release, m.latest.snapshot, m.versions.len()).into());
        ui.set_versions(ModelRc::new(VecModel::from(rows)));
        let releases: Vec<String> = m.versions.iter().filter(|v| v.kind == "release").map(|v| v.id.clone()).collect();
        let labels: Vec<SharedString> = releases.iter().map(|r| r.as_str().into()).collect();
        ui.set_mc_releases(ModelRc::new(VecModel::from(labels)));
        self.mc.borrow_mut().releases = releases;
    }

    fn mc_refresh_mods(&self) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<McUi>();
        let Some(p) = self.mc_selected() else {
            ui.set_mods(ModelRc::new(VecModel::from(Vec::<McModRow>::new())));
            return;
        };
        let layout = self.mc_layout();
        let st = self.mc.borrow();
        let rows: Vec<McModRow> = mods::scan(&layout, &p)
            .into_iter()
            .map(|m| {
                let info = m.info.clone();
                McModRow {
                    file: m.file.as_str().into(),
                    name: info.as_ref().map(|i| i.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| m.file.trim_end_matches(".jar").to_string()).into(),
                    version: info.as_ref().map(|i| i.version.clone()).unwrap_or_default().into(),
                    source: match info.as_ref().map(|i| i.source.as_str()) {
                        Some("modrinth") => "Modrinth",
                        Some("curseforge") => "CurseForge",
                        Some("modpack") => "Modpack",
                        _ => "Ajouté à la main",
                    }
                    .into(),
                    enabled: m.enabled,
                    dependency: info.as_ref().is_some_and(|i| i.dependency),
                    update: st.updates.iter().find(|(f, _)| f == &m.file).map(|(_, v)| v.number.clone()).unwrap_or_default().into(),
                }
            })
            .collect();
        ui.set_updates(st.updates.len() as i32);
        ui.set_mods(ModelRc::new(VecModel::from(rows)));
        drop(st);
        let installed: Vec<String> = p.mods.iter().map(|m| m.project_id.clone()).collect();
        self.mc_publish_projects(false, &installed);
    }

    fn mc_publish_projects(&self, packs: bool, installed: &[String]) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<McUi>();
        let icons = self.mc_layout().cache().join("icons");
        let st = self.mc.borrow();
        let list = if packs { &st.packs } else { &st.results };
        let rows: Vec<McProjectRow> = list
            .iter()
            .map(|p| {
                let icon_path = icons.join(icon_name(p));
                // Décodage par contenu (PNG, JPEG, WebP), pas par extension.
                let img = icon_path.is_file().then(|| crate::images::load(&icon_path, crate::images::Kind::Icon, &icons.join("thumbs")).ok()).flatten().map(Image::from_rgba8);
                McProjectRow {
                    id: p.id.as_str().into(),
                    title: p.title.as_str().into(),
                    description: p.description.as_str().into(),
                    author: p.author.as_str().into(),
                    downloads: fmt_count(p.downloads).into(),
                    has_icon: img.is_some(),
                    icon: img.unwrap_or_default(),
                    source: if p.source == Source::Modrinth { 0 } else { 1 },
                    url: p.page_url.as_str().into(),
                    installed: !packs && installed.contains(&p.id),
                }
            })
            .collect();
        if packs {
            ui.set_packs(ModelRc::new(VecModel::from(rows)));
        } else {
            ui.set_results(ModelRc::new(VecModel::from(rows)));
        }
    }

    // ------------------------------------------------------------ opérations de fond

    fn mc_set_status(&self, task: &str) {
        if let Some(app) = self.app() {
            let ui = app.global::<McUi>();
            ui.set_task(task.into());
            ui.set_progress(-1.0);
            ui.set_progress_text("".into());
            ui.set_log_line("".into());
            ui.set_error("".into());
            ui.set_info("".into());
        }
    }

    /// Lance `work` sur un fil ; progression réelle ; `work` renvoie l'action finale.
    fn mc_run(&self, title: &str, work: impl FnOnce(&Ctx) -> Result<Done, Error> + Send + 'static) {
        if self.mc.borrow().busy {
            return self.toast("Une opération Minecraft est déjà en cours", true);
        }
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut st = self.mc.borrow_mut();
            st.busy = true;
            st.cancel = cancel.clone();
        }
        self.mc_set_status(title);
        if let Some(app) = self.app() {
            app.global::<McUi>().set_busy(true);
        }
        let task = self.task_start("minecraft", title.to_string());
        let ctx = Ctx {
            http: Http::new(),
            layout: self.mc_layout(),
            cancel,
            manifest: self.mc.borrow().manifest.clone(),
            java_path: self.settings.borrow().minecraft.java_path.trim().to_string(),
        };
        let title = title.to_string();
        std::thread::Builder::new()
            .name("halcyon-minecraft".into())
            .spawn(move || {
                // File d'attente commune du gestionnaire de téléchargements.
                let transfers = halcyon_core::transfer::global();
                if transfers.active() >= transfers.max_concurrent() {
                    post(move |c| c.task_update(task, Some(-1.0), Some("En attente dans la file des téléchargements…")));
                }
                let slot = transfers.acquire(&ctx.cancel);
                let result = if slot.is_none() {
                    Err(Error::Cancelled)
                } else {
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(&ctx))).unwrap_or_else(|_| Err(Error::Invalid("erreur interne".into())))
                };
                drop(slot);
                post(move |c| {
                    c.mc.borrow_mut().busy = false;
                    match result {
                        Ok(done) => {
                            c.task_finish(task, true, format!("{title} : terminé"), crate::tasks::Action::OpenPage("minecraft"));
                            done(c);
                        }
                        Err(Error::Cancelled) => {
                            c.task_finish(task, false, "Annulé", crate::tasks::Action::None);
                            c.mc_info("Opération annulée.");
                        }
                        Err(e) => {
                            let m = msg(&e);
                            log::warn!("Minecraft : {title} : {m}");
                            c.task_finish(task, false, m.clone(), crate::tasks::Action::OpenPage("minecraft"));
                            if let Some(app) = c.app() {
                                app.global::<McUi>().set_error(m.into());
                            }
                            c.sound(halcyon_audio::Cue::Error);
                        }
                    }
                    c.mc_reload_profiles();
                    c.mc_refresh_ui();
                    c.mc_sync_library();
                });
            })
            .ok();
    }

    fn mc_info(&self, text: &str) {
        if let Some(app) = self.app() {
            app.global::<McUi>().set_info(text.into());
        }
    }

    // ------------------------------------------------------------------- versions

    pub(crate) fn mc_install_version(&self, id: &str) {
        let id = id.to_string();
        let repair = install::installed(&self.mc_layout()).contains(&id);
        self.mc_run(&format!("{} de Minecraft {id}", if repair { "Réparation" } else { "Installation" }), move |ctx| {
            let v = install::install(&ctx.http, &ctx.layout, &id, ctx.manifest.as_deref(), repair, &ctx.cancel, &ctx.sink())?;
            ctx.java(&v)?;
            let p = profiles::for_version(&ctx.layout, &id, Loader::Vanilla, "", &id);
            profiles::save(&ctx.layout, &p).map_err(|e| Error::Disk(e.to_string()))?;
            let name = p.name.clone();
            Ok(Box::new(move |c: &Rc<Controller>| {
                c.mc_info(&format!("{name} est prête : onglet Jouer."));
                c.sound(halcyon_audio::Cue::Install);
            }))
        });
    }

    pub(crate) fn mc_verify_version(&self, id: &str) {
        let id = id.to_string();
        self.mc_run(&format!("Vérification de {id}"), move |ctx| {
            let bad = install::verify(&ctx.layout, &id)?;
            Ok(Box::new(move |c: &Rc<Controller>| {
                if bad.is_empty() {
                    c.mc_info(&format!("{id} : tous les fichiers sont présents et intacts."));
                } else {
                    c.mc_info(&format!("{id} : {} fichier(s) manquant(s) ou endommagé(s). « Réparer » les télécharge à nouveau.", bad.len()));
                }
            }))
        });
    }

    pub(crate) fn mc_remove_version(&self, id: &str) {
        let used: Vec<String> = self.mc.borrow().profiles.iter().filter(|p| p.version_id == id || p.mc == id && p.loader != Loader::Vanilla).map(|p| p.name.clone()).collect();
        let vanilla_only = self.mc.borrow().profiles.iter().any(|p| p.version_id == id && p.loader == Loader::Vanilla && p.modpack.is_none());
        if !used.is_empty() && !(used.len() == 1 && vanilla_only) {
            return self.toast(format!("Utilisée par : {}. Supprimez d'abord ces installations.", used.join(", ")), true);
        }
        let id = id.to_string();
        self.confirm(
            &format!("Supprimer Minecraft {id} ?"),
            "Les fichiers de cette version sont supprimés (bibliothèques et assets partagés conservés). Vos mondes restent dans leur installation.",
            "Supprimer",
            move |c| match install::remove(&c.mc_layout(), &id) {
                Ok(()) => {
                    c.mc_refresh_ui();
                    c.toast(format!("Minecraft {id} supprimée"), false);
                }
                Err(e) => c.toast(msg(&e), true),
            },
        );
    }

    // -------------------------------------------------------------------- loaders

    pub(crate) fn mc_loader_changed(&self) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<McUi>();
        let releases = self.mc.borrow().releases.clone();
        let Some(mc) = releases.get(ui.get_loader_mc().max(0) as usize).cloned() else { return };
        let loader = LOADER_ORDER[ui.get_loader_kind().clamp(0, 3) as usize];
        ui.set_loader_info(format!("Recherche des versions de {} pour Minecraft {mc}…", loader.name()).into());
        ui.set_loader_versions(ModelRc::new(VecModel::from(Vec::<SharedString>::new())));
        if !self.net_allowed() {
            ui.set_loader_info("Hors ligne : la liste des versions des loaders est indisponible.".into());
            return;
        }
        std::thread::spawn(move || {
            let r = loaders::versions(&Http::new(), loader, &mc);
            post(move |c| {
                let Some(app) = c.app() else { return };
                let ui = app.global::<McUi>();
                match r {
                    Ok(list) => {
                        let labels: Vec<SharedString> = list.iter().map(|v| SharedString::from(if v.stable { format!("{} (stable)", v.version) } else { v.version.clone() })).collect();
                        let first_stable = list.iter().position(|v| v.stable).unwrap_or(0);
                        ui.set_loader_info(
                            if list.is_empty() { format!("{} ne propose pas de version pour Minecraft {mc}.", loader.name()) } else { format!("{} version(s) de {} compatibles avec Minecraft {mc}.", list.len(), loader.name()) }.into(),
                        );
                        ui.set_loader_versions(ModelRc::new(VecModel::from(labels)));
                        ui.set_loader_version(first_stable as i32);
                        c.mc.borrow_mut().loader_versions = list;
                    }
                    Err(e) => ui.set_loader_info(msg(&e).into()),
                }
            });
        });
    }

    pub(crate) fn mc_install_loader(&self) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<McUi>();
        let st = self.mc.borrow();
        let Some(mc) = st.releases.get(ui.get_loader_mc().max(0) as usize).cloned() else { return };
        let Some(lv) = st.loader_versions.get(ui.get_loader_version().max(0) as usize).map(|v| v.version.clone()) else { return };
        drop(st);
        let loader = LOADER_ORDER[ui.get_loader_kind().clamp(0, 3) as usize];
        self.mc_run(&format!("Installation de {} {lv} pour Minecraft {mc}", loader.name()), move |ctx| {
            let id = ctx.install_stack(&mc, loader, &lv)?;
            let p = profiles::for_version(&ctx.layout, &mc, loader, &lv, &id);
            profiles::save(&ctx.layout, &p).map_err(|e| Error::Disk(e.to_string()))?;
            mods::ensure_dirs(&ctx.layout, &p);
            let name = p.name.clone();
            Ok(Box::new(move |c: &Rc<Controller>| {
                c.mc_info(&format!("{name} est prête. Ajoutez des mods dans l'onglet Mods."));
                c.sound(halcyon_audio::Cue::Install);
            }))
        });
    }

    // ------------------------------------------------------------------------ jouer

    pub(crate) fn mc_play(&self) {
        let Some(p) = self.mc_selected() else { return };
        let s = self.settings.borrow().minecraft.clone();
        {
            let mut set = self.settings.borrow_mut();
            set.minecraft.last_profile = p.id.clone();
        }
        self.save_settings();
        let opts = Options {
            min_ram_mb: s.ram_min_mb,
            max_ram_mb: s.ram_max_mb,
            jvm_args: s.jvm_args.clone(),
            width: s.width,
            height: s.height,
            fullscreen: s.fullscreen,
        };
        if s.play_mode != "direct" {
            let Some(off) = official::dir() else {
                return self.mc_error("Launcher officiel introuvable : installez-le (minecraft.net) ou passez en mode direct (Paramètres).");
            };
            let Some((kind, exe)) = official::launcher() else {
                return self.mc_error("Launcher officiel introuvable : installez-le (minecraft.net) ou passez en mode direct (Paramètres).");
            };
            if !s.official_consent {
                return self.mc_error("Autorisez d'abord Halcyon à ajouter ses installations au launcher officiel (encadré Compte).");
            }
            let java_args = format!("-Xms{}M -Xmx{}M {}", opts.min_ram_mb, opts.max_ram_mb, opts.jvm_args).trim().to_string();
            let res = (opts.width > 0 && opts.height > 0).then_some((opts.width, opts.height));
            self.mc_run(&format!("Préparation de {}", p.name), move |ctx| {
                // Fichiers complets côté Halcyon (loaders, bibliothèques produites par Forge…).
                install::install(&ctx.http, &ctx.layout, &p.version_id, ctx.manifest.as_deref(), false, &ctx.cancel, &ctx.sink())?;
                official::export_profile(&ctx.layout, &off, &p, &java_args, res).map_err(|e| Error::Disk(e.to_string()))?;
                halcyon_storage::audit::event("minecraft.official_profile.write", &official::profile_key(&p));
                official::open(kind, &exe).map_err(|e| Error::Invalid(format!("launcher officiel non lancé : {e}")))?;
                let name = p.name.clone();
                Ok(Box::new(move |c: &Rc<Controller>| {
                    if let Some(id) = c.mc_library_id() {
                        let _ = c.db().mark_launched(id, profiles::now());
                    }
                    c.mc_info(&format!("Launcher officiel ouvert : choisissez le profil « Halcyon · {name} » (sélectionné en dernier) puis Jouer."));
                    c.sound(halcyon_audio::Cue::Launch);
                }))
            });
            return;
        }
        // Mode direct : compte Microsoft de Halcyon.
        let account = self.mc.borrow().account.clone();
        let Some(account) = account else {
            return self.mc_error("Connectez votre compte Microsoft (encadré Compte) avant de jouer.");
        };
        let client_id = s.client_id.trim().to_string();
        self.mc_run(&format!("Lancement de {}", p.name), move |ctx| {
            let account = if account.expires_at <= profiles::now() {
                ctx.sink()(Event::Step("Renouvellement de la session Microsoft".into()));
                let store = auth::Store::new(ctx.layout.root.join("accounts"));
                let refresh = store.refresh_token(&account.uuid).ok_or_else(|| Error::Invalid("session expirée : reconnectez-vous".into()))?;
                let a = Auth { http: &ctx.http, client_id: &client_id };
                let (new_refresh, ms) = a.refresh(&refresh)?;
                let fresh = a.minecraft(&ms)?;
                store.save(&fresh, &new_refresh).map_err(|e| Error::Disk(e.to_string()))?;
                fresh
            } else {
                account
            };
            let v = install::install(&ctx.http, &ctx.layout, &p.version_id, ctx.manifest.as_deref(), false, &ctx.cancel, &ctx.sink())?;
            let java = ctx.java(&v)?;
            ctx.sink()(Event::Step("Natives et assets".into()));
            let natives = install::extract_natives(&ctx.layout, &v)?;
            let game = p.game_dir(&ctx.layout);
            install::prepare_legacy_assets(&ctx.layout, &v, Some(&game))?;
            let who = Identity { name: account.name.clone(), uuid: account.uuid.clone(), access_token: account.access_token.clone(), xuid: account.xuid.clone() };
            let cmd = launch::command(&ctx.layout, &v, &game, &java, &natives, &who, &opts);
            log::info!("Minecraft : {}", cmd.redacted());
            let log_path = game.join("logs").join("halcyon-launch.log");
            let mut child = launch::spawn(&cmd, &log_path).map_err(|e| Error::Invalid(format!("Java n'a pas pu démarrer : {e}")))?;
            let pid = child.id();
            let started = std::time::Instant::now();
            let profile_id = p.id.clone();
            let layout = ctx.layout.clone();
            post(move |c| c.mc_session(true, &format!("En jeu (processus {pid})")));
            std::thread::spawn(move || {
                let status = child.wait();
                let secs = started.elapsed().as_secs();
                let failed = status.as_ref().map(|s| !s.success()).unwrap_or(true) && secs < 60;
                let tail = if failed { launch::log_tail(&log_path, 12) } else { String::new() };
                if let Some(mut pr) = profiles::load(&layout, &profile_id) {
                    pr.last_played = profiles::now();
                    pr.played_secs += secs as i64;
                    let _ = profiles::save(&layout, &pr);
                }
                let started_at = profiles::now() - secs as i64;
                post(move |c| {
                    if let Some(id) = c.mc_library_id() {
                        let db = c.db();
                        let _ = db.mark_launched(id, started_at);
                        let _ = db.record_session(id, started_at, secs);
                    }
                    c.mc_session(false, &format!("Partie terminée ({} min)", secs / 60));
                    if failed {
                        c.mc_error(&format!("Minecraft s'est arrêté au démarrage. Fin du journal :\n{tail}"));
                    }
                    c.mc_reload_profiles();
                    c.mc_refresh_ui();
                    c.mc_sync_library();
                });
            });
            Ok(Box::new(|c: &Rc<Controller>| c.sound(halcyon_audio::Cue::Launch)))
        });
    }

    fn mc_session(&self, on: bool, status: &str) {
        self.mc.borrow_mut().playing = on;
        self.audio.set_game_active(on);
        if let Some(app) = self.app() {
            let ui = app.global::<McUi>();
            ui.set_playing(on);
            ui.set_play_status(status.into());
        }
    }

    fn mc_error(&self, text: &str) {
        if let Some(app) = self.app() {
            app.global::<McUi>().set_error(text.into());
        }
        self.sound(halcyon_audio::Cue::Error);
    }

    pub(crate) fn mc_delete_profile(&self) {
        let Some(p) = self.mc_selected() else { return };
        let worlds = profiles::worlds(&p.game_dir(&self.mc_layout()));
        let detail = if worlds.is_empty() {
            "Son dossier (mods, réglages) est supprimé. Les versions téléchargées restent disponibles.".to_string()
        } else {
            format!("Son dossier est supprimé, y compris {} monde(s) : {}. Cette suppression est définitive.", worlds.len(), worlds.join(", "))
        };
        self.confirm(&format!("Supprimer « {} » ?", p.name), &detail, "Supprimer", move |c| {
            let layout = c.mc_layout();
            if let Some(off) = official::dir() {
                if official::remove_profile(&off, &p).is_ok() {
                    halcyon_storage::audit::event("minecraft.official_profile.remove", &official::profile_key(&p));
                }
            }
            match profiles::delete(&layout, &p.id) {
                Ok(()) => c.toast(format!("{} supprimée", p.name), false),
                Err(e) => c.toast(e.to_string(), true),
            }
            c.mc_reload_profiles();
            c.mc_refresh_ui();
            c.mc_sync_library();
        });
    }

    // ---------------------------------------------------------------------- compte

    fn mc_restore_account(&self) {
        let store = auth::Store::new(self.mc_layout().root.join("accounts"));
        let (accounts, active) = store.accounts();
        let a = active.and_then(|id| accounts.into_iter().find(|a| a.uuid == id));
        // Jeton Minecraft absent : renouvelé au lancement.
        self.mc.borrow_mut().account = a;
    }

    pub(crate) fn mc_login(&self) {
        let client_id = self.settings.borrow().minecraft.client_id.trim().to_string();
        let cancel = Arc::new(AtomicBool::new(false));
        self.mc.borrow_mut().cancel = cancel.clone();
        let layout = self.mc_layout();
        if let Some(app) = self.app() {
            let ui = app.global::<McUi>();
            ui.set_error("".into());
            ui.set_logging_in(true);
            ui.set_login_code("…".into());
        }
        std::thread::spawn(move || {
            let http = Http::new();
            let a = Auth { http: &http, client_id: &client_id };
            let code = match a.start() {
                Ok(c) => c,
                Err(e) => {
                    let m = msg(&e);
                    return post(move |c| c.mc_login_done(Err(m)));
                }
            };
            let (uc, url) = (code.user_code.clone(), code.verification_uri.clone());
            post(move |c| {
                if let Some(app) = c.app() {
                    let ui = app.global::<McUi>();
                    ui.set_login_code(uc.into());
                    ui.set_login_url(url.as_str().into());
                }
                c.open_url(&url);
            });
            let r = a.wait(&code, &cancel).and_then(|(refresh, ms)| {
                let acc = a.minecraft(&ms)?;
                auth::Store::new(layout.root.join("accounts")).save(&acc, &refresh).map_err(|e| Error::Disk(e.to_string()))?;
                Ok(acc)
            });
            let r = r.map_err(|e| if e == Error::Cancelled { String::new() } else { msg(&e) });
            post(move |c| c.mc_login_done(r));
        });
    }

    fn mc_login_done(&self, r: Result<Account, String>) {
        if let Some(app) = self.app() {
            app.global::<McUi>().set_logging_in(false);
        }
        match r {
            Ok(a) => {
                halcyon_storage::audit::event("minecraft.login", &a.name);
                self.toast(format!("Connecté : {}", a.name), false);
                self.mc.borrow_mut().account = Some(a);
            }
            Err(m) if m.is_empty() => {}
            Err(m) => self.mc_error(&m),
        }
        self.mc_refresh_ui();
    }

    pub(crate) fn mc_logout(&self) {
        let Some(a) = self.mc.borrow_mut().account.take() else { return };
        let _ = auth::Store::new(self.mc_layout().root.join("accounts")).remove(&a.uuid);
        halcyon_storage::audit::event("minecraft.logout", &a.name);
        self.toast("Compte déconnecté de Halcyon", false);
        self.mc_refresh_ui();
    }

    // ------------------------------------------------------------------------ mods

    pub(crate) fn mc_search(&self, query: &str, packs: bool) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<McUi>();
        let source = if ui.get_source() == 1 { Source::CurseForge } else { Source::Modrinth };
        let key = self.settings.borrow().curseforge_api_key.clone();
        let profile = self.mc_selected();
        if !packs && profile.as_ref().is_some_and(|p| p.loader == Loader::Vanilla) {
            ui.set_results_info("Installation vanilla : choisissez une installation avec mod loader (Fabric, Quilt, Forge, NeoForge).".into());
            return;
        }
        let (mc, loader) = match (&profile, packs) {
            (Some(p), false) => (Some(p.mc.clone()), Some(p.loader)),
            _ => (None, None),
        };
        let info = format!("Recherche{}…", mc.as_ref().map(|m| format!(" pour Minecraft {m}")).unwrap_or_default());
        if packs {
            ui.set_packs_info(info.into());
        } else {
            ui.set_results_info(info.into());
        }
        let query = query.to_string();
        let icons = self.mc_layout().cache().join("icons");
        std::thread::spawn(move || {
            let http = Http::new();
            let c = mods::Client { http: &http, curseforge_key: &key };
            let r = c.search(source, if packs { Kind::Modpack } else { Kind::Mod }, &query, mc.as_deref(), loader, 0);
            let r2 = r.clone();
            post(move |ctl| {
                let Some(app) = ctl.app() else { return };
                let ui = app.global::<McUi>();
                let text = match &r {
                    Ok((list, total)) => format!("{} résultat(s){}", total, if list.is_empty() { " : rien de compatible" } else { "" }),
                    Err(e) => msg(e),
                };
                let installed: Vec<String> = ctl.mc_selected().map(|p| p.mods.iter().map(|m| m.project_id.clone()).collect()).unwrap_or_default();
                {
                    let mut st = ctl.mc.borrow_mut();
                    let list = r.map(|x| x.0).unwrap_or_default();
                    if packs {
                        st.packs = list;
                    } else {
                        st.results = list;
                    }
                }
                if packs {
                    ui.set_packs_info(text.into());
                } else {
                    ui.set_results_info(text.into());
                }
                ctl.mc_publish_projects(packs, &installed);
            });
            // Icônes : téléchargées une fois, puis affichées depuis le cache.
            if let Ok((list, _)) = r2 {
                let _ = std::fs::create_dir_all(icons.join("thumbs"));
                for p in &list {
                    if let Some(u) = &p.icon_url {
                        let dest = icons.join(icon_name(p));
                        if !dest.is_file() {
                            let _ = http.download(u, &dest, None, &AtomicBool::new(false), &mut |_| {});
                        }
                    }
                }
                post(move |ctl| {
                    let installed: Vec<String> = ctl.mc_selected().map(|p| p.mods.iter().map(|m| m.project_id.clone()).collect()).unwrap_or_default();
                    ctl.mc_publish_projects(packs, &installed);
                });
            }
        });
    }

    pub(crate) fn mc_install_project(&self, index: usize) {
        let Some(project) = self.mc.borrow().results.get(index).cloned() else { return };
        let Some(mut p) = self.mc_selected() else { return };
        let key = self.settings.borrow().curseforge_api_key.clone();
        self.mc_run(&format!("Installation de {}", project.title), move |ctx| {
            let c = mods::Client { http: &ctx.http, curseforge_key: &key };
            let mut v = c
                .best(project.source, &project.id, &p.mc, p.loader)?
                .ok_or_else(|| Error::Invalid(format!("{} n'a pas de version pour Minecraft {} avec {}", project.title, p.mc, p.loader.name())))?;
            v.name = format!("{} {}", project.title, v.number);
            let added = c.install_mod(&ctx.layout, &mut p, v, &ctx.cancel, &ctx.sink())?;
            Ok(Box::new(move |ctl: &Rc<Controller>| {
                let deps = added.iter().filter(|m| m.dependency).count();
                ctl.mc_info(&if deps > 0 { format!("{} installé, avec {deps} dépendance(s).", project.title) } else { format!("{} installé.", project.title) });
                ctl.sound(halcyon_audio::Cue::Install);
            }))
        });
    }

    pub(crate) fn mc_mod_toggle(&self, file: &str, on: bool) {
        let Some(p) = self.mc_selected() else { return };
        if let Err(e) = mods::set_enabled(&self.mc_layout(), &p, file, on) {
            self.toast(e.to_string(), true);
        }
        self.mc_refresh_mods();
    }

    pub(crate) fn mc_mod_remove(&self, file: &str) {
        let Some(mut p) = self.mc_selected() else { return };
        let file = file.to_string();
        self.confirm(&format!("Supprimer {file} ?"), "Le fichier du mod est supprimé de cette installation.", "Supprimer", move |c| {
            if let Err(e) = mods::remove(&c.mc_layout(), &mut p, &file) {
                c.toast(e.to_string(), true);
            }
            c.mc_reload_profiles();
            c.mc_refresh_ui();
        });
    }

    /// Recherche des mises à jour des mods Modrinth de l'installation sélectionnée.
    pub(crate) fn mc_check_updates(&self) {
        let Some(p) = self.mc_selected() else { return };
        if !self.net_allowed() || p.mods.iter().all(|m| m.source != "modrinth") {
            self.mc.borrow_mut().updates.clear();
            return;
        }
        let layout = self.mc_layout();
        std::thread::spawn(move || {
            let r = mods::modrinth_updates(&Http::new(), &layout, &p).unwrap_or_default();
            post(move |c| {
                c.mc.borrow_mut().updates = r;
                c.mc_refresh_mods();
            });
        });
    }

    pub(crate) fn mc_update_mods(&self) {
        let Some(mut p) = self.mc_selected() else { return };
        let updates = self.mc.borrow().updates.clone();
        let key = self.settings.borrow().curseforge_api_key.clone();
        self.mc_run(&format!("Mise à jour de {} mod(s)", updates.len()), move |ctx| {
            let c = mods::Client { http: &ctx.http, curseforge_key: &key };
            for (file, mut v) in updates {
                let name = p.mods.iter().find(|m| m.file == file).map(|m| m.name.split(' ').next().unwrap_or("").to_string()).unwrap_or_default();
                v.name = format!("{name} {}", v.number).trim().to_string();
                c.install_mod(&ctx.layout, &mut p, v, &ctx.cancel, &ctx.sink())?;
            }
            Ok(Box::new(|ctl: &Rc<Controller>| {
                ctl.mc.borrow_mut().updates.clear();
                ctl.mc_info("Mods mis à jour.");
                ctl.sound(halcyon_audio::Cue::Update);
            }))
        });
    }

    // -------------------------------------------------------------------- modpacks

    pub(crate) fn mc_install_pack(&self, index: usize) {
        let Some(project) = self.mc.borrow().packs.get(index).cloned() else { return };
        let key = self.settings.borrow().curseforge_api_key.clone();
        self.mc_run(&format!("Installation du modpack {}", project.title), move |ctx| {
            let c = mods::Client { http: &ctx.http, curseforge_key: &key };
            let versions = c.versions(project.source, &project.id, None, None)?;
            let v = versions.iter().find(|v| v.channel == "release").or(versions.first()).cloned().ok_or_else(|| Error::Invalid("modpack sans version publiée".into()))?;
            let url = v.file.url.clone().ok_or_else(|| Error::Invalid("l'auteur de ce modpack n'autorise pas son téléchargement hors de CurseForge".into()))?;
            if !mods::safe_name(&v.file.filename) {
                return Err(Error::Invalid("nom de fichier refusé".into()));
            }
            let file = ctx.layout.cache().join("packs").join(&v.file.filename);
            ctx.sink()(Event::Step(format!("Téléchargement de {}", v.file.filename)));
            ctx.http.download(&url, &file, v.file.sha1.as_deref(), &ctx.cancel, &mut |_| {})?;
            let reference = ModpackRef { source: project.source.id().into(), project_id: project.id.clone(), version_id: v.id.clone(), name: project.title.clone(), version: v.number.clone(), files: vec![] };
            ctx.install_pack_file(&file, Some(reference), &key, None)
        });
    }

    pub(crate) fn mc_import_pack(&self) {
        let Some(path) = crate::dialogs::pick_modpack() else { return };
        let key = self.settings.borrow().curseforge_api_key.clone();
        self.mc_run("Import d'un modpack", move |ctx| ctx.install_pack_file(&path, None, &key, None));
    }

    pub(crate) fn mc_update_pack(&self) {
        let Some(p) = self.mc_selected() else { return };
        let Some(r) = p.modpack.clone() else { return };
        if r.source != "modrinth" {
            return self.toast("Mise à jour automatique disponible pour les modpacks Modrinth ; réimportez la nouvelle version des autres modpacks", true);
        }
        self.mc_run(&format!("Mise à jour de {}", p.name), move |ctx| {
            let c = mods::Client { http: &ctx.http, curseforge_key: "" };
            let versions = c.versions(Source::Modrinth, &r.project_id, None, None)?;
            let v = versions.iter().find(|v| v.channel == "release").or(versions.first()).cloned().ok_or_else(|| Error::Invalid("aucune version publiée".into()))?;
            if v.id == r.version_id {
                return Ok(Box::new(|ctl: &Rc<Controller>| ctl.mc_info("Le modpack est déjà à jour.")));
            }
            let file = ctx.layout.cache().join("packs").join(&v.file.filename);
            ctx.http.download(v.file.url.as_deref().unwrap_or_default(), &file, v.file.sha1.as_deref(), &ctx.cancel, &mut |_| {})?;
            let reference = ModpackRef { version_id: v.id.clone(), version: v.number.clone(), ..r };
            ctx.install_pack_file(&file, Some(reference), "", Some(p))
        });
    }

    pub(crate) fn mc_export_pack(&self) {
        let Some(p) = self.mc_selected() else { return };
        let Some(out) = crate::dialogs::save_file(&format!("{}.mrpack", p.id), "mrpack") else { return };
        self.mc_run(&format!("Export de {}", p.name), move |ctx| {
            let n = modpack::export_mrpack(&ctx.http, &ctx.layout, &p, &out, "1.0.0")?;
            Ok(Box::new(move |c: &Rc<Controller>| c.mc_info(&format!("Exporté : {} ({n} mod(s) référencé(s) sur Modrinth, le reste inclus dans l'archive).", out.display()))))
        });
    }

    // --------------------------------------------------------------------- réglages

    pub(crate) fn mc_set_text(&self, key: &str, value: &str) {
        let v = value.trim();
        let num = |s: &str| s.parse::<u32>().ok();
        {
            let mut s = self.settings.borrow_mut();
            let m = &mut s.minecraft;
            match key {
                "client_id" => m.client_id = v.to_string(),
                "ram_min" => match num(v) {
                    Some(n) => m.ram_min_mb = n,
                    None => return self.toast("Indiquez une valeur en Mo (ex. 1024)", true),
                },
                "ram_max" => match num(v) {
                    Some(n) => m.ram_max_mb = n,
                    None => return self.toast("Indiquez une valeur en Mo (ex. 4096)", true),
                },
                "java_path" => {
                    if !v.is_empty() && java::probe(Path::new(v.trim_matches('"'))).is_none() {
                        return self.toast("Ce fichier n'est pas un exécutable Java valide", true);
                    }
                    m.java_path = v.trim_matches('"').to_string();
                }
                "jvm_args" => m.jvm_args = v.to_string(),
                "width" => m.width = num(v).unwrap_or(0),
                "height" => m.height = num(v).unwrap_or(0),
                "curseforge_key" => s.curseforge_api_key = v.to_string(),
                _ => return,
            }
            s.minecraft.sanitize();
        }
        self.save_settings();
        self.mc_refresh_ui();
        self.toast("Enregistré", false);
    }

    pub(crate) fn mc_set_bool(&self, key: &str, value: bool) {
        {
            let mut s = self.settings.borrow_mut();
            match key {
                "show_snapshots" => s.minecraft.show_snapshots = value,
                "show_old" => s.minecraft.show_old = value,
                "fullscreen" => s.minecraft.fullscreen = value,
                _ => return,
            }
        }
        self.save_settings();
        self.mc_refresh_ui();
    }

    pub(crate) fn mc_set_int(&self, key: &str, value: i32) {
        match key {
            "play_mode" => {
                self.settings.borrow_mut().minecraft.play_mode = if value == 1 { "direct".into() } else { "official".into() };
                self.save_settings();
                self.mc_refresh_ui();
            }
            "source" => {
                if let Some(app) = self.app() {
                    app.global::<McUi>().set_source(value);
                }
            }
            _ => {}
        }
    }

    pub(crate) fn mc_consent(&self, on: bool) {
        self.settings.borrow_mut().minecraft.official_consent = on;
        self.save_settings();
        self.mc_refresh_ui();
    }

    pub(crate) fn mc_open_folder(&self, what: &str) {
        let layout = self.mc_layout();
        let dir = match (what, self.mc_selected()) {
            ("game", Some(p)) => p.game_dir(&layout),
            ("mods", Some(p)) => {
                mods::ensure_dirs(&layout, &p);
                p.mods_dir(&layout)
            }
            _ => layout.root.clone(),
        };
        let _ = std::fs::create_dir_all(&dir);
        self.open_folder(&dir.display().to_string());
    }

    pub(crate) fn mc_java_list(&self) {
        let layout = self.mc_layout();
        std::thread::spawn(move || {
            let list = java::detect(&layout);
            post(move |c| {
                if let Some(app) = c.app() {
                    let rows: Vec<SharedString> = list.iter().map(|j| SharedString::from(format!("Java {} ({}, {}) — {}", j.major, j.version, j.source, j.path.display()))).collect();
                    app.global::<McUi>().set_java_list(ModelRc::new(VecModel::from(rows)));
                }
            });
        });
    }

    pub(crate) fn mc_set_tab(&self, tab: i32) {
        if let Some(app) = self.app() {
            let ui = app.global::<McUi>();
            ui.set_tab(tab);
            // Le message d'une opération terminée ne suit pas d'un onglet à l'autre.
            if !self.mc.borrow().busy {
                ui.set_info("".into());
            }
        }
        match tab {
            2 => self.mc_check_updates(),
            4 => {
                if self.mc.borrow().loader_versions.is_empty() {
                    self.mc_loader_changed();
                }
            }
            5 => self.mc_java_list(),
            _ => {}
        }
        self.mc_refresh_ui();
    }

    pub(crate) fn wire_minecraft(self: &Rc<Self>, app: &crate::AppWindow) {
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
        let ui = app.global::<McUi>();
        ui.on_set_tab(with!(|c, t| c.mc_set_tab(t)));
        ui.on_refresh(with!(|c| {
            c.mc_fetch_manifest();
            c.mc_refresh_ui();
        }));
        ui.on_cancel(with!(|c| {
            c.mc.borrow().cancel.store(true, Ordering::SeqCst);
        }));
        ui.on_dismiss(with!(|c| {
            if let Some(app) = c.app() {
                let ui = app.global::<McUi>();
                ui.set_error("".into());
                ui.set_info("".into());
            }
        }));
        ui.on_login(with!(|c| c.mc_login()));
        ui.on_logout(with!(|c| c.mc_logout()));
        ui.on_consent(with!(|c, v| c.mc_consent(v)));
        ui.on_open_official(with!(|c| {
            if let Some((k, p)) = official::launcher() {
                if let Err(e) = official::open(k, &p) {
                    c.toast(e.to_string(), true);
                }
            }
        }));
        ui.on_select_profile(with!(|c, i| {
            if let Some(app) = c.app() {
                app.global::<McUi>().set_selected_profile(i);
            }
            c.mc.borrow_mut().updates.clear();
            c.mc_refresh_ui();
            c.mc_check_updates();
        }));
        ui.on_play(with!(|c| c.mc_play()));
        ui.on_open_folder(with!(|c, w| c.mc_open_folder(&w)));
        ui.on_delete_profile(with!(|c| c.mc_delete_profile()));
        ui.on_install_version(with!(|c, id| c.mc_install_version(&id)));
        ui.on_remove_version(with!(|c, id| c.mc_remove_version(&id)));
        ui.on_verify_version(with!(|c, id| c.mc_verify_version(&id)));
        ui.on_filter_versions(with!(|c| c.mc_refresh_versions()));
        ui.on_search(with!(|c, q| c.mc_search(&q, false)));
        ui.on_install_project(with!(|c, i| c.mc_install_project(i.max(0) as usize)));
        ui.on_mod_toggle(with!(|c, f, v| c.mc_mod_toggle(&f, v)));
        ui.on_mod_remove(with!(|c, f| c.mc_mod_remove(&f)));
        ui.on_update_mods(with!(|c| c.mc_update_mods()));
        ui.on_search_packs(with!(|c, q| c.mc_search(&q, true)));
        ui.on_install_pack(with!(|c, i| c.mc_install_pack(i.max(0) as usize)));
        ui.on_import_pack(with!(|c| c.mc_import_pack()));
        ui.on_export_pack(with!(|c| c.mc_export_pack()));
        ui.on_update_pack(with!(|c| c.mc_update_pack()));
        ui.on_loader_changed(with!(|c| c.mc_loader_changed()));
        ui.on_install_loader(with!(|c| c.mc_install_loader()));
        ui.on_set_text(with!(|c, k, v| c.mc_set_text(&k, &v)));
        ui.on_set_bool(with!(|c, k, v| c.mc_set_bool(&k, v)));
        ui.on_set_int(with!(|c, k, v| c.mc_set_int(&k, v)));
        ui.on_open_url(with!(|c, u| c.open_url(&u)));
    }
}

/// Source « Minecraft » de la bibliothèque : une entrée « Minecraft: Java Edition » dès qu'une
/// installation Halcyon ou le launcher officiel est présent. Aucun accès réseau.
pub(crate) struct McScanner {
    pub layout: Layout,
}

pub(crate) const LIBRARY_KEY: &str = "java-edition";

impl halcyon_core::scanner::GameScanner for McScanner {
    fn source(&self) -> halcyon_core::GameSource {
        halcyon_core::GameSource::Minecraft
    }

    fn scan(&self, _ctx: &halcyon_core::scanner::ScanContext) -> halcyon_core::Result<halcyon_core::scanner::ScanReport> {
        Ok(halcyon_core::scanner::ScanReport { games: library_entry(&self.layout).into_iter().collect(), warnings: Vec::new(), complete: true })
    }
}

fn library_entry(layout: &Layout) -> Option<halcyon_core::DiscoveredGame> {
    let list = profiles::list(layout);
    if list.is_empty() && official::launcher().is_none() {
        return None;
    }
    let mut d = halcyon_core::DiscoveredGame::new(LIBRARY_KEY, "Minecraft: Java Edition");
    d.developer = Some("Mojang Studios".into());
    d.publisher = Some("Mojang Studios".into());
    d.release_date = Some("2011-11-18".into());
    d.genres = vec!["Bac à sable".into(), "Survie".into()];
    d.description = Some(match list.len() {
        0 => "Aucune installation Halcyon pour l'instant : Jouer ouvre l'onglet Minecraft.".to_string(),
        n => format!("{n} installation(s) dans l'onglet Minecraft : {}.", list.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", ")),
    });
    d.install_dir = Some(layout.root.clone());
    // Temps mesuré par Halcyon en mode direct (le launcher officiel ne le communique pas).
    let played: i64 = list.iter().map(|p| p.played_secs).sum();
    d.playtime_secs = (played > 0).then_some(played as u64);
    d.last_played = list.iter().map(|p| p.last_played).filter(|t| *t > 0).max();
    Some(d)
}

impl Controller {
    /// Met à jour l'entrée Minecraft de la bibliothèque après un changement d'installations.
    pub(crate) fn mc_sync_library(&self) {
        let report = halcyon_core::scanner::ScanReport { games: library_entry(&self.mc_layout()).into_iter().collect(), warnings: Vec::new(), complete: true };
        let games = {
            let mut db = self.db();
            if let Err(e) = db.merge_scan(halcyon_core::GameSource::Minecraft, &report) {
                return log::warn!("Minecraft : bibliothèque non mise à jour : {e}");
            }
            db.load_games()
        };
        if let Ok(g) = games {
            self.reload_games(g);
            self.refresh_all();
        }
    }

    fn mc_library_id(&self) -> Option<halcyon_core::GameId> {
        self.lib.borrow().games.iter().find(|g| g.source == halcyon_core::GameSource::Minecraft && g.source_key == LIBRARY_KEY).map(|g| g.id)
    }

    /// Bouton Jouer de la bibliothèque, de l'accueil ou de Big Picture.
    pub(crate) fn mc_play_from_library(&self, _id: halcyon_core::GameId) {
        // En Big Picture, on reste en plein écran : seuls les messages s'affichent.
        let big_picture = self.bp.borrow().active;
        if !big_picture {
            if let Some(app) = self.app() {
                app.global::<crate::Nav>().invoke_go(crate::Page::Minecraft);
            }
        }
        self.mc_reload_profiles();
        if self.mc.borrow().profiles.is_empty() {
            self.mc_refresh_ui();
            let text = "Installez d'abord une version (Versions), un loader (Loaders) ou un modpack (Modpacks) dans l'onglet Minecraft.";
            self.mc_info(text);
            if big_picture {
                self.toast(text, true);
            }
            return;
        }
        if self.mc.borrow().playing {
            return self.toast("Minecraft est déjà en cours", false);
        }
        self.mc_play();
    }
}

fn icon_name(p: &Project) -> String {
    format!("{}-{}.img", p.source.id(), p.id.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>())
}

fn fmt_count(n: u64) -> String {
    match n {
        n if n >= 1_000_000 => format!("{:.1} M", n as f64 / 1e6),
        n if n >= 1_000 => format!("{:.0} k", n as f64 / 1e3),
        n => n.to_string(),
    }
}

/// Contexte d'une opération de fond (aucune référence à l'interface).
pub(crate) struct Ctx {
    pub http: Http,
    pub layout: Layout,
    pub cancel: Arc<AtomicBool>,
    pub manifest: Option<Arc<Manifest>>,
    pub java_path: String,
}

impl Ctx {
    /// Progression envoyée à l'interface (déjà limitée à ~10 rafraîchissements par seconde).
    pub fn sink(&self) -> impl Fn(Event) + Sync {
        |e: Event| {
            post(move |c| {
                let Some(app) = c.app() else { return };
                let ui = app.global::<McUi>();
                match e {
                    Event::Step(s) => {
                        ui.set_task(s.into());
                        ui.set_progress(-1.0);
                        ui.set_progress_text("".into());
                    }
                    Event::Progress { bytes, total_bytes, files, total_files } => {
                        if total_files > 0 {
                            let p = if total_bytes > 0 { bytes as f32 / total_bytes as f32 } else { files as f32 / total_files as f32 };
                            ui.set_progress(p.clamp(0.0, 1.0));
                            ui.set_progress_text(format!("{files} / {total_files} fichiers · {} / {}", mods::human(bytes), mods::human(total_bytes)).into());
                        }
                    }
                    Event::Log(l) => ui.set_log_line(l.chars().take(200).collect::<String>().into()),
                }
            });
        }
    }

    /// Java pour une version : celui choisi dans les paramètres, sinon le runtime officiel.
    pub fn java(&self, v: &halcyon_minecraft::meta::Version) -> Result<PathBuf, Error> {
        if !self.java_path.is_empty() {
            let p = PathBuf::from(&self.java_path);
            let (_, major) = java::probe(&p).ok_or_else(|| Error::Invalid(format!("Java choisi introuvable : {}", p.display())))?;
            if major < v.java_major() {
                return Err(Error::Invalid(format!("Minecraft {} demande Java {} ; le Java choisi est la version {major} (laissez le champ vide pour le Java officiel)", v.id, v.java_major())));
            }
            return Ok(p);
        }
        let comp = v.java_component();
        match java::mojang_runtime(&self.layout, &comp) {
            Some(p) => Ok(p),
            None => java::install_runtime(&self.http, &self.layout, &comp, &self.cancel, &self.sink()),
        }
    }

    /// Vanilla + Java + loader ; renvoie l'identifiant de la version à lancer.
    pub fn install_stack(&self, mc: &str, loader: Loader, lv: &str) -> Result<String, Error> {
        let sink = self.sink();
        let vanilla = install::install(&self.http, &self.layout, mc, self.manifest.as_deref(), false, &self.cancel, &sink)?;
        let java = self.java(&vanilla)?;
        if loader == Loader::Vanilla {
            return Ok(mc.to_string());
        }
        let id = loaders::install(&self.http, &self.layout, loader, mc, lv, Some(&java), &self.cancel, &sink)?;
        install::install(&self.http, &self.layout, &id, self.manifest.as_deref(), false, &self.cancel, &sink)?;
        Ok(id)
    }

    /// Modpack depuis un fichier (`.mrpack` ou CurseForge), puis loader et version.
    pub fn install_pack_file(&self, file: &Path, reference: Option<ModpackRef>, cf_key: &str, existing: Option<Profile>) -> Result<Done, Error> {
        let sink = self.sink();
        let is_mrpack = modpack::read_mrpack(file).is_ok();
        let (mut profile, manual) = if is_mrpack {
            let r = reference.unwrap_or(ModpackRef { source: "file".into(), project_id: String::new(), version_id: String::new(), name: String::new(), version: String::new(), files: vec![] });
            (modpack::install_mrpack(&self.http, &self.layout, file, r, existing, &self.cancel, &sink)?, Vec::new())
        } else {
            modpack::install_curseforge(&self.http, cf_key, &self.layout, file, &self.cancel, &sink)?
        };
        let id = self.install_stack(&profile.mc.clone(), profile.loader, &profile.loader_version.clone())?;
        profile.version_id = id;
        profiles::save(&self.layout, &profile).map_err(|e| Error::Disk(e.to_string()))?;
        let name = profile.name.clone();
        let count = profile.mods.len();
        Ok(Box::new(move |c: &Rc<Controller>| {
            if manual.is_empty() {
                c.mc_info(&format!("Modpack {name} installé ({count} mods) : onglet Jouer."));
            } else {
                c.mc_info(&format!(
                    "Modpack {name} installé ; {} fichier(s) que leurs auteurs n'autorisent pas à télécharger hors de CurseForge sont à déposer dans le dossier mods : {}",
                    manual.len(),
                    manual.join(", ")
                ));
            }
            c.sound(halcyon_audio::Cue::Install);
        }))
    }
}

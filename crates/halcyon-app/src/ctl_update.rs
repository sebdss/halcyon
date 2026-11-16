//! Halcyon Update System dans le lanceur : recherche (canal, ETag), téléchargement en
//! arrière-plan, mise en attente vérifiée, lancement de l'updater, contrôle de santé au
//! redémarrage et compte rendu à l'utilisateur. L'installation elle-même est faite par
//! `halcyon-updater`, Halcyon fermé.

use crate::controller::{post, Controller};
use crate::{AppUi, HusUi};
use halcyon_core::util;
use halcyon_storage::Settings;
use halcyon_update::client::{self, Kind};
use halcyon_update::download::{self, DownloadError, Progress, Retry};
use halcyon_update::install;
use halcyon_update::network::{self, Net, Outcome, Query};
use halcyon_update::recovery::{self, Layout};
use halcyon_update::runner::{self, Startup};
use halcyon_update::{security, Channel, Platform, State, Version};
use slint::ComponentHandle;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Intervalle entre deux recherches automatiques.
const CHECK_EVERY: Duration = Duration::from_secs(6 * 3600);

pub(crate) struct Hus {
    kind: Kind,
    found: Option<Box<network::Found>>,
    /// Version publiée mais non vérifiable (compilation sans clé) : signalée seulement.
    unverified: Option<(String, String)>,
    cancel: Option<Arc<AtomicBool>>,
    state: State,
    detail: String,
    error: bool,
    checking: bool,
    progress: Option<Progress>,
    timer: Option<slint::Timer>,
}

impl Default for Hus {
    fn default() -> Self {
        Hus { kind: Kind::Unsupported(String::new()), found: None, unverified: None, cancel: None, state: State::Idle, detail: String::new(), error: false, checking: false, progress: None, timer: None }
    }
}

fn channel_of(s: &Settings) -> Channel {
    s.update_channel.as_deref().and_then(Channel::parse).unwrap_or_else(|| Channel::default_for(&Version::current()))
}

fn repo_of(s: &Settings) -> String {
    let r = s.update_repo.trim();
    if r.is_empty() { network::OFFICIAL_REPO.to_string() } else { r.to_string() }
}

/// Avant la création de la fenêtre : reprise d'une installation interrompue, ou installation
/// d'une mise à jour en attente. `true` : l'updater a pris le relais, Halcyon doit se fermer.
pub(crate) fn early_start(settings: &Settings, health: Option<&str>, rolled_back: bool) -> bool {
    let Kind::Managed { layout, target, updater } = client::detect() else { return false };
    if health.is_none() && runner::on_launcher_start(&layout, None) == Startup::NeedsRecovery {
        log::warn!("mise à jour interrompue : reprise par l'updater");
        match client::launch_recover(&layout, &updater, &target) {
            Ok(()) => return true,
            Err(e) => log::error!("reprise impossible : {e}"),
        }
    }
    if health.is_some() || rolled_back {
        return false;
    }
    let Some(p) = install::read_pending(&layout) else { return false };
    let record = recovery::load(&layout);
    let wanted = settings.update_auto || client::install_at_restart(&layout);
    let version_ok = p.version.parse::<Version>().is_ok_and(|v| channel_of(settings).accepts(&v, v.is_prerelease()) && v > Version::current());
    if record.state != State::Staging || record.blocked.as_deref() == Some(p.version.as_str()) || !wanted || !version_ok {
        return false;
    }
    log::info!("installation de la mise à jour {} au démarrage", p.version);
    match client::launch_apply(&layout, &updater, &Version::current()) {
        Ok(()) => true,
        Err(e) => {
            log::error!("updater non lancé : {e}");
            false
        }
    }
}

/// Dossier d'installation utilisateur de l'AppImage.
fn user_appimage_dir() -> Option<std::path::PathBuf> {
    dirs::home_dir().map(|h| h.join(".local/opt/halcyon"))
}

/// AppImage en cours, si elle n'est pas déjà dans `~/.local/opt/halcyon` (Linux).
fn appimage_outside_user_dir() -> Option<std::path::PathBuf> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let img = std::env::var_os("APPIMAGE").map(std::path::PathBuf::from).filter(|p| p.is_file())?;
    let dir = user_appimage_dir()?;
    (img.parent() != Some(dir.as_path())).then_some(img)
}

/// Installation de l'AppImage pour l'utilisateur : copie atomique, icône, entrée de menu,
/// lien dans `~/.local/bin`. Sans droits administrateur.
#[cfg(target_os = "linux")]
fn install_appimage_for_user(src: &std::path::Path) -> Result<std::path::PathBuf, String> {
    use std::os::unix::fs::PermissionsExt;
    let home = dirs::home_dir().ok_or("dossier personnel introuvable")?;
    let dir = home.join(".local/opt/halcyon");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{} : {e}", dir.display()))?;
    let dest = dir.join(install::APPIMAGE_FILE);
    let tmp = dir.join(".Halcyon.AppImage.tmp");
    std::fs::copy(src, &tmp).map_err(|e| format!("copie : {e}"))?;
    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    security::check_appimage(&tmp)?;
    std::fs::rename(&tmp, &dest).map_err(|e| e.to_string())?;
    let data = dirs::data_dir().unwrap_or_else(|| home.join(".local/share"));
    let icon_name = match std::env::var_os("APPDIR").map(|d| std::path::PathBuf::from(d).join("halcyon.png")).filter(|p| p.is_file()) {
        Some(icon) => {
            let to = data.join("icons/hicolor/256x256/apps/halcyon.png");
            let _ = std::fs::create_dir_all(to.parent().unwrap_or(&data));
            if std::fs::copy(icon, &to).is_ok() { "halcyon".to_string() } else { dest.display().to_string() }
        }
        None => dest.display().to_string(),
    };
    let apps = data.join("applications");
    std::fs::create_dir_all(&apps).map_err(|e| e.to_string())?;
    std::fs::write(apps.join("halcyon.desktop"), halcyon_core::desktop::app_entry(&dest.display().to_string(), &icon_name)).map_err(|e| e.to_string())?;
    let bin = dirs::executable_dir().unwrap_or_else(|| home.join(".local/bin"));
    let link = bin.join("halcyon");
    let _ = std::fs::create_dir_all(&bin);
    // Un exécutable réel déjà présent (autre installation) n'est jamais remplacé.
    if link.symlink_metadata().map(|m| m.file_type().is_symlink()).unwrap_or(true) {
        let _ = std::fs::remove_file(&link);
        let _ = std::os::unix::fs::symlink(&dest, &link);
    }
    Ok(dest)
}

impl Controller {
    #[cfg(target_os = "linux")]
    pub(crate) fn hus_install_for_user(self: &Rc<Self>) {
        let Some(src) = appimage_outside_user_dir() else { return };
        match install_appimage_for_user(&src) {
            Ok(dest) => self.confirm(
                "Halcyon est installé pour votre compte",
                &format!("Halcyon est maintenant dans {} et dans le menu des applications. Redémarrer depuis cet emplacement ? Le fichier téléchargé peut ensuite être supprimé.", dest.display()),
                "Redémarrer",
                move |c| {
                    // L'instance unique est libérée d'abord : la nouvelle ne s'y renvoie pas.
                    c.quit();
                    if let Err(e) = std::process::Command::new(&dest).spawn() {
                        log::error!("démarrage de {} impossible : {e}", dest.display());
                    }
                },
            ),
            Err(e) => self.toast(format!("Installation impossible : {e}"), true),
        }
    }

    #[cfg(not(target_os = "linux"))]
    pub(crate) fn hus_install_for_user(self: &Rc<Self>) {}

    fn hus_layout(&self) -> Option<Layout> {
        match &self.hus.borrow().kind {
            Kind::Managed { layout, .. } => Some(layout.clone()),
            _ => None,
        }
    }

    /// Au démarrage (fenêtre affichée) : compte rendu de la dernière installation, contrôle
    /// de santé, recherche automatique.
    pub(crate) fn hus_init(self: &Rc<Self>, health: Option<String>) {
        self.hus.borrow_mut().kind = client::detect();
        if let Some(layout) = self.hus_layout() {
            if let Some(tx) = health {
                // La fenêtre est affichée et la boucle d'évènements tourne : démarrage réussi.
                let weak = Rc::downgrade(self);
                slint::Timer::single_shot(Duration::from_secs(3), move || {
                    let result = runner::on_launcher_start(&layout, Some(&tx));
                    log::info!("contrôle de santé de la mise à jour : {result:?}");
                    if let Some(c) = weak.upgrade() {
                        c.hus_report();
                    }
                });
            } else {
                self.hus_report();
            }
        }
        self.hus_refresh_ui();
        let s = self.settings.borrow().clone();
        if s.check_updates {
            self.hus_check(false);
        }
        let weak = Rc::downgrade(self);
        let timer = slint::Timer::default();
        timer.start(slint::TimerMode::Repeated, CHECK_EVERY, move || {
            if let Some(c) = weak.upgrade() {
                if c.settings.borrow().check_updates {
                    c.hus_check(false);
                }
            }
        });
        self.hus.borrow_mut().timer = Some(timer);
    }

    /// Résultat de la dernière installation, signalé une seule fois.
    fn hus_report(self: &Rc<Self>) {
        let Some(layout) = self.hus_layout() else { return };
        let mut r = recovery::load(&layout);
        if r.acknowledged {
            return;
        }
        match r.state {
            State::Completed => {
                self.toast(format!("Halcyon {} est installé", r.to), false);
                halcyon_storage::audit::event("update.installed", &format!("{} → {}", r.from, r.to));
                let _ = self.db().log_activity(halcyon_storage::ActivityKind::Settings, None, "Mise à jour de Halcyon", &format!("{} → {}", r.from, r.to));
            }
            State::RolledBack | State::Failed => {
                self.toast(r.message.clone(), true);
                halcyon_storage::audit::event("update.failed", &r.message);
                let mut h = self.hus.borrow_mut();
                h.state = r.state;
                h.detail = r.message.clone();
                h.error = true;
            }
            _ => return,
        }
        r.acknowledged = true;
        let _ = recovery::save(&layout, &r);
        self.hus_refresh_ui();
    }

    pub(crate) fn hus_refresh_ui(&self) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<HusUi>();
        let s = self.settings.borrow();
        let h = self.hus.borrow();
        let layout = self.hus_layout();
        let pending = layout.as_ref().and_then(install::read_pending);
        let staged = pending.is_some() && layout.as_ref().is_some_and(|l| recovery::load(l).state == State::Staging);
        ui.set_installed(env!("CARGO_PKG_VERSION").into());
        ui.set_channel(if channel_of(&s) == Channel::Beta { 1 } else { 0 });
        ui.set_channel_default(s.update_channel.is_none());
        ui.set_auto(s.update_auto);
        ui.set_managed(h.kind.managed());
        ui.set_kind(h.kind.describe().into());
        let keys = security::trusted_keys();
        ui.set_verification(
            if keys.is_empty() {
                "Cette compilation n'intègre aucune clé de signature : les nouvelles versions sont signalées, jamais installées automatiquement.".to_string()
            } else {
                format!("Manifeste signé Ed25519 (clé {}), empreinte SHA-256 du paquet, contrôle du paquet avant installation, aucun retour vers une version plus ancienne.", keys.iter().map(security::fingerprint).collect::<Vec<_>>().join(", "))
            }
            .into(),
        );
        ui.set_last_check(if s.update_last_check > 0 { format!("{}, {}", util::format_relative(s.update_last_check, halcyon_storage::now_unix()), util::format_date(s.update_last_check)).into() } else { "Jamais".into() });
        let latest = h.found.as_ref().map(|f| f.checked.version.to_string()).or_else(|| h.unverified.as_ref().map(|u| u.0.clone())).or_else(|| pending.as_ref().map(|p| p.version.clone())).unwrap_or_default();
        ui.set_latest(latest.as_str().into());
        ui.set_notes(h.found.as_ref().map(|f| f.checked.manifest.notes.clone()).unwrap_or_default().into());
        ui.set_release_url(h.found.as_ref().map(|f| f.release_url.clone()).or_else(|| h.unverified.as_ref().map(|u| u.1.clone())).unwrap_or_default().into());
        ui.set_security(h.found.as_ref().is_some_and(|f| f.security));
        let state = if staged && !matches!(h.state, State::Downloading | State::Verifying) { State::Staging } else { h.state };
        ui.set_state(state.code());
        let status = match state {
            State::Available if h.unverified.is_some() && h.found.is_none() => format!("Version {latest} publiée"),
            State::Available => format!("Version {latest} disponible"),
            State::Staging => format!("Version {latest} prête à être installée"),
            State::UpToDate => format!("Halcyon {} est à jour", env!("CARGO_PKG_VERSION")),
            State::Idle => "Aucune recherche effectuée".to_string(),
            other => other.label().to_string(),
        };
        ui.set_status(status.into());
        ui.set_detail(h.detail.as_str().into());
        ui.set_error(h.error);
        ui.set_checking(h.checking);
        ui.set_downloading(h.cancel.is_some());
        ui.set_can_download(h.found.is_some() && h.kind.managed() && h.cancel.is_none() && !staged);
        ui.set_staged(staged);
        ui.set_at_restart(staged && (s.update_auto || layout.as_ref().is_some_and(client::install_at_restart)));
        let p = h.progress.unwrap_or_default();
        ui.set_progress(if h.cancel.is_some() && p.total > 0 { p.ratio() } else { -1.0 });
        ui.set_progress_text(if p.total > 0 { format!("{} %", (p.ratio() * 100.0).floor() as u32).into() } else { "".into() });
        ui.set_speed(if p.speed > 0 { download::format_speed(p.speed).into() } else { "".into() });
        ui.set_eta(p.eta.map(|e| format!("{} restantes", download::format_eta(e))).unwrap_or_default().into());
        ui.set_size_text(if p.total > 0 { format!("{} sur {}", download::format_bytes(p.done), download::format_bytes(p.total)).into() } else { "".into() });
        ui.set_can_install_user(appimage_outside_user_dir().is_some());
        let shell = app.global::<AppUi>();
        let announce = if h.found.is_some() || h.unverified.is_some() || staged { latest } else { String::new() };
        shell.set_update_version(announce.as_str().into());
    }

    /// Recherche d'une mise à jour sur le canal choisi.
    pub(crate) fn hus_check(self: &Rc<Self>, manual: bool) {
        if !self.net_allowed() {
            if manual {
                self.toast("Hors ligne : recherche des mises à jour impossible pour l'instant", true);
            }
            return;
        }
        if self.hus.borrow().checking || self.hus.borrow().cancel.is_some() {
            return;
        }
        let Some(platform) = Platform::current() else {
            if manual {
                self.toast("Mises à jour automatiques indisponibles sur ce système", true);
            }
            return;
        };
        let s = self.settings.borrow().clone();
        let (repo, channel) = (repo_of(&s), channel_of(&s));
        let layout = self.hus_layout();
        let floor = layout.as_ref().map(recovery::load).and_then(|r| [r.highest, r.security_floor].into_iter().flatten().filter_map(|v| v.parse::<Version>().ok()).max());
        let cache = layout.as_ref().map(|l| l.hus().join("releases.json")).unwrap_or_else(|| self.paths.cache_dir.join("hus-releases.json"));
        {
            let mut h = self.hus.borrow_mut();
            h.checking = true;
            h.state = State::Checking;
            h.detail.clear();
            h.error = false;
        }
        self.hus_refresh_ui();
        std::thread::Builder::new()
            .name("hus-check".into())
            .spawn(move || {
                let keys = security::trusted_keys();
                let current = Version::current();
                let net = Net::new();
                let result = if keys.is_empty() {
                    // Sans clé : la version publiée est signalée, jamais installée.
                    network::fetch_releases(&net, &repo, &cache).map(|list| {
                        let best = halcyon_update::core::candidates(&list, channel, &current).first().map(|r| (r.version.to_string(), r.html_url.clone()));
                        Err::<Outcome, _>(best)
                    })
                } else {
                    let q = Query { repo: &repo, channel, current: &current, platform, keys: &keys, floor: floor.as_ref().filter(|f| **f > current), cache };
                    network::check(&net, &q).map(Ok)
                };
                post(move |c| c.hus_checked(result, manual));
            })
            .ok();
    }

    #[allow(clippy::type_complexity)]
    fn hus_checked(self: &Rc<Self>, result: Result<Result<Outcome, Option<(String, String)>>, String>, manual: bool) {
        {
            let mut s = self.settings.borrow_mut();
            s.update_last_check = halcyon_storage::now_unix();
        }
        self.save_settings();
        let mut start_download = false;
        let blocked = self.hus_layout().and_then(|l| recovery::load(&l).blocked);
        let message;
        {
            let mut h = self.hus.borrow_mut();
            h.checking = false;
            h.unverified = None;
            match result {
                Ok(Ok(Outcome::Available(found))) => {
                    let v = found.checked.version.to_string();
                    h.state = State::Available;
                    h.detail = if found.security { "Mise à jour de sécurité recommandée".into() } else { String::new() };
                    if blocked.as_deref() == Some(v.as_str()) {
                        h.detail = format!("La dernière tentative d'installation de {v} a échoué : installation manuelle uniquement");
                    }
                    start_download = self.settings.borrow().update_auto && h.kind.managed() && blocked.as_deref() != Some(v.as_str());
                    message = format!("Version {v} disponible");
                    h.found = Some(found);
                }
                Ok(Ok(Outcome::UpToDate { note })) => {
                    h.state = State::UpToDate;
                    h.found = None;
                    h.detail = note.unwrap_or_default();
                    message = format!("Halcyon {} est à jour", env!("CARGO_PKG_VERSION"));
                }
                Ok(Err(Some(unverified))) => {
                    h.state = State::Available;
                    h.found = None;
                    h.detail = "Compilation sans clé de signature : téléchargez cette version depuis sa page GitHub".into();
                    message = format!("Version {} publiée", unverified.0);
                    h.unverified = Some(unverified);
                }
                Ok(Err(None)) => {
                    h.state = State::UpToDate;
                    h.found = None;
                    message = format!("Halcyon {} est à jour", env!("CARGO_PKG_VERSION"));
                }
                Err(e) => {
                    if e.contains("signature") {
                        halcyon_storage::audit::event("update.rejected", &e);
                    }
                    log::warn!("recherche de mise à jour : {e}");
                    h.state = State::Failed;
                    h.error = true;
                    h.detail = format!("Recherche impossible : {e}");
                    message = h.detail.clone();
                }
            }
        }
        self.hus_refresh_ui();
        if manual {
            self.toast(message, self.hus.borrow().error);
        }
        if start_download {
            self.hus_download();
        }
    }

    /// Téléchargement en arrière-plan, puis vérification et mise en attente.
    pub(crate) fn hus_download(self: &Rc<Self>) {
        let (found, layout, target) = {
            let h = self.hus.borrow();
            let (Some(f), Kind::Managed { layout, target, .. }) = (h.found.clone(), &h.kind) else { return };
            if h.cancel.is_some() {
                return;
            }
            (f, layout.clone(), target.clone())
        };
        let version = found.checked.version.clone();
        if install::read_pending(&layout).is_some_and(|p| p.version == version.to_string()) && recovery::load(&layout).state == State::Staging {
            self.hus_refresh_ui();
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut h = self.hus.borrow_mut();
            h.cancel = Some(cancel.clone());
            h.state = State::Downloading;
            h.detail.clear();
            h.error = false;
            h.progress = None;
        }
        let task = self.task_start("hus", format!("Mise à jour de Halcyon {version}"));
        self.hus_refresh_ui();
        std::thread::Builder::new()
            .name("hus-download".into())
            .spawn(move || {
                let net = Net::new();
                let mut on = |p: Progress| post(move |c| c.hus_progress(task, p));
                let result = download::download(&net, &found.package_url, &found.package, &layout.downloads(), &cancel, Retry::default(), &mut on).and_then(|path| {
                    post(|c| {
                        c.hus.borrow_mut().state = State::Verifying;
                        c.hus_refresh_ui();
                    });
                    security::check_package(&path, found.package.format).map_err(DownloadError::Failed)?;
                    install::stage(&layout, &found.manifest_bytes, &found.signature, &path, &found.checked.version, &target).map_err(DownloadError::Failed)
                });
                post(move |c| c.hus_downloaded(task, &version, result));
            })
            .ok();
    }

    fn hus_progress(&self, task: u64, p: Progress) {
        self.hus.borrow_mut().progress = Some(p);
        self.task_update(task, Some(p.ratio()), Some(&format!("{} sur {}", download::format_bytes(p.done), download::format_bytes(p.total))));
        self.hus_refresh_ui();
    }

    fn hus_downloaded(self: &Rc<Self>, task: u64, version: &Version, result: Result<(), DownloadError>) {
        {
            let mut h = self.hus.borrow_mut();
            h.cancel = None;
            h.progress = None;
            match &result {
                Ok(()) => {
                    h.state = State::Staging;
                    h.detail = String::new();
                }
                Err(DownloadError::Cancelled) => {
                    h.state = State::Available;
                    h.detail = "Téléchargement annulé : il reprendra où il s'est arrêté".into();
                }
                Err(DownloadError::Failed(e)) => {
                    h.state = State::Failed;
                    h.error = true;
                    h.detail = format!("Téléchargement impossible : {e}");
                }
            }
        }
        let action = crate::tasks::Action::OpenPage("settings");
        match result {
            Ok(()) => {
                halcyon_storage::audit::event("update.verified", &format!("{version} : signature, SHA-256 et paquet vérifiés"));
                self.task_finish(task, true, "Téléchargée et vérifiée", action);
                let auto = self.settings.borrow().update_auto;
                self.toast(if auto { format!("Halcyon {version} est prêt : il sera installé au prochain démarrage") } else { format!("Halcyon {version} est prêt à être installé") }, false);
            }
            Err(DownloadError::Cancelled) => self.task_finish(task, false, "Annulé", action),
            Err(DownloadError::Failed(e)) => {
                if e.contains("SHA-256") {
                    halcyon_storage::audit::event("update.rejected", &e);
                }
                self.task_finish(task, false, e, action);
            }
        }
        self.hus_refresh_ui();
    }

    pub(crate) fn hus_cancel(&self) {
        if let Some(c) = &self.hus.borrow().cancel {
            c.store(true, Ordering::SeqCst);
        }
    }

    /// Installe maintenant : l'updater prend le relais et Halcyon se ferme.
    pub(crate) fn hus_install_now(self: &Rc<Self>) {
        let Some(version) = self.hus_layout().and_then(|l| install::read_pending(&l)).map(|p| p.version) else {
            return self.toast("Aucune mise à jour prête à être installée", true);
        };
        self.confirm(
            &format!("Installer Halcyon {version} ?"),
            "Halcyon va se fermer, installer la mise à jour vérifiée puis redémarrer. Si la nouvelle version ne démarre pas correctement, la version actuelle est restaurée automatiquement. Vos jeux, paramètres et sauvegardes ne sont pas modifiés.",
            "Installer et redémarrer",
            |c| {
                let (layout, updater) = match &c.hus.borrow().kind {
                    Kind::Managed { layout, updater, .. } => (layout.clone(), updater.clone()),
                    _ => return,
                };
                match client::launch_apply(&layout, &updater, &Version::current()) {
                    Ok(()) => c.quit(),
                    Err(e) => c.toast(format!("Installation impossible : {e}"), true),
                }
            },
        );
    }

    pub(crate) fn hus_install_at_restart(&self) {
        let Some(layout) = self.hus_layout() else { return };
        match client::set_install_at_restart(&layout, true) {
            Ok(()) => self.toast("La mise à jour sera installée au prochain démarrage de Halcyon", false),
            Err(e) => self.toast(format!("Choix non enregistré : {e}"), true),
        }
        self.hus_refresh_ui();
    }

    pub(crate) fn hus_set_channel(self: &Rc<Self>, index: i32) {
        let channel = if index == 1 { Channel::Beta } else { Channel::Stable };
        self.settings.borrow_mut().update_channel = Some(channel.as_str().into());
        self.save_settings();
        // Une prépublication en attente n'est plus installée après un passage en Stable.
        if let Some(layout) = self.hus_layout() {
            if let Some(p) = install::read_pending(&layout) {
                if p.version.parse::<Version>().is_ok_and(|v| !channel.accepts(&v, v.is_prerelease())) {
                    install::discard_pending(&layout);
                    let mut r = recovery::load(&layout);
                    r.state = State::Idle;
                    let _ = recovery::save(&layout, &r);
                    recovery::log(&layout, &format!("canal {} : mise à jour {} en attente abandonnée", channel.as_str(), p.version));
                }
            }
            recovery::log(&layout, &format!("canal choisi par l'utilisateur : {}", channel.as_str()));
        }
        self.hus.borrow_mut().found = None;
        self.hus_refresh_ui();
        self.hus_check(true);
    }

    pub(crate) fn hus_set_auto(self: &Rc<Self>, on: bool) {
        self.settings.borrow_mut().update_auto = on;
        self.save_settings();
        self.hus_refresh_ui();
        if on && self.hus.borrow().found.is_some() {
            self.hus_download();
        }
    }

    pub(crate) fn hus_open_release(&self) {
        let url = {
            let h = self.hus.borrow();
            h.found.as_ref().map(|f| f.release_url.clone()).or_else(|| h.unverified.as_ref().map(|u| u.1.clone()))
        };
        let url = url.unwrap_or_else(|| format!("https://github.com/{}/releases", repo_of(&self.settings.borrow())));
        self.open_url(&url);
    }

    pub(crate) fn wire_hus(self: &Rc<Self>, app: &crate::AppWindow) {
        let ui = app.global::<HusUi>();
        let w = Rc::downgrade(self);
        macro_rules! cb {
            ($method:ident, |$c:ident $(, $a:ident)*| $body:expr) => {{
                let w = w.clone();
                ui.$method(move |$($a),*| {
                    if let Some($c) = w.upgrade() {
                        $body
                    }
                });
            }};
        }
        cb!(on_check, |c| c.hus_check(true));
        cb!(on_download, |c| c.hus_download());
        cb!(on_cancel, |c| c.hus_cancel());
        cb!(on_install_now, |c| c.hus_install_now());
        cb!(on_install_at_restart, |c| c.hus_install_at_restart());
        cb!(on_set_channel, |c, i| c.hus_set_channel(i));
        cb!(on_set_auto, |c, v| c.hus_set_auto(v));
        cb!(on_open_release, |c| c.hus_open_release());
        cb!(on_install_user, |c| c.hus_install_for_user());
    }
}

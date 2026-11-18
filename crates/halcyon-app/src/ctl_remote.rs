//! Jeu à distance : pilote Sunshine (serveur) et Moonlight (client) existants.
//! Aucun protocole de diffusion n'est réimplémenté.

use crate::controller::{post, Controller};
use crate::{HostRow, RemoteUi};
use halcyon_core::model::GameId;
use halcyon_integrations::remote_play::{self, Tool};
use halcyon_storage::settings::RemoteHost;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::path::PathBuf;
use std::rc::Rc;

/// État détecté (calculé hors du fil de l'interface).
#[derive(Clone, Default)]
pub struct RemoteState {
    pub moonlight: Option<Tool>,
    pub sunshine: Option<Tool>,
    pub sunshine_running: bool,
    pub apps_file: Option<PathBuf>,
    pub online: Vec<bool>,
}

impl Controller {
    fn remote_status(&self, msg: impl Into<SharedString>) {
        if let Some(app) = self.app() {
            app.global::<RemoteUi>().set_status(msg.into());
        }
    }

    fn remote_busy(&self, on: bool) {
        if let Some(app) = self.app() {
            app.global::<RemoteUi>().set_busy(on);
        }
    }

    /// Détection des logiciels et disponibilité des machines (en arrière-plan : connexions réseau).
    pub(crate) fn refresh_remote(&self) {
        let forced = self.settings.borrow().moonlight_path.clone();
        let hosts = self.settings.borrow().remote_hosts.clone();
        self.remote_busy(true);
        std::thread::Builder::new()
            .name("halcyon-remote".into())
            .spawn(move || {
                let state = RemoteState {
                    moonlight: remote_play::detect_moonlight(forced.as_deref()),
                    sunshine: remote_play::detect_sunshine(),
                    sunshine_running: remote_play::host_reachable("127.0.0.1"),
                    apps_file: remote_play::sunshine_apps_file(),
                    online: hosts.iter().map(|h| remote_play::host_reachable(&h.address)).collect(),
                };
                post(move |c| {
                    *c.remote_state.borrow_mut() = state;
                    c.remote_busy(false);
                    c.refresh_remote_ui();
                });
            })
            .ok();
    }

    pub(crate) fn refresh_remote_ui(&self) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<RemoteUi>();
        let st = self.remote_state.borrow().clone();
        ui.set_moonlight(st.moonlight.is_some());
        ui.set_moonlight_via(st.moonlight.as_ref().map(|t| format!("{} ({})", t.command.join(" "), t.via)).unwrap_or_default().into());
        ui.set_sunshine(st.sunshine.is_some());
        ui.set_sunshine_via(st.sunshine.as_ref().map(|t| t.via.clone()).unwrap_or_default().into());
        ui.set_sunshine_running(st.sunshine_running);
        ui.set_apps_file(st.apps_file.as_ref().map(|p| p.display().to_string()).unwrap_or_default().into());
        let hosts = self.settings.borrow().remote_hosts.clone();
        let rows: Vec<HostRow> = hosts
            .iter()
            .enumerate()
            .map(|(i, h)| HostRow { name: h.name.clone().into(), address: h.address.clone().into(), online: st.online.get(i).copied().unwrap_or(false) })
            .collect();
        if ui.get_selected_host() >= rows.len() as i32 {
            ui.set_selected_host(-1);
            ui.set_apps(ModelRc::default());
        }
        ui.set_hosts(ModelRc::new(VecModel::from(rows)));
    }

    pub(crate) fn add_remote_host(&self, name: &str, address: &str) {
        let address = address.trim().to_string();
        if address.is_empty() || address.contains(char::is_whitespace) {
            self.remote_status(tr!("Adresse invalide"));
            return;
        }
        let name = if name.trim().is_empty() { address.clone() } else { name.trim().to_string() };
        {
            let mut s = self.settings.borrow_mut();
            if s.remote_hosts.iter().any(|h| h.address.eq_ignore_ascii_case(&address)) {
                drop(s);
                self.remote_status(tr!("Cette machine est déjà dans la liste"));
                return;
            }
            s.remote_hosts.push(RemoteHost { name, address });
        }
        self.save_settings();
        self.refresh_remote();
    }

    pub(crate) fn remove_remote_host(&self, index: usize) {
        {
            let mut s = self.settings.borrow_mut();
            if index < s.remote_hosts.len() {
                s.remote_hosts.remove(index);
            }
        }
        self.save_settings();
        if let Some(app) = self.app() {
            app.global::<RemoteUi>().set_selected_host(-1);
            app.global::<RemoteUi>().set_apps(ModelRc::default());
        }
        self.refresh_remote();
    }

    fn host_and_tool(&self, index: usize) -> Option<(RemoteHost, Tool)> {
        let host = self.settings.borrow().remote_hosts.get(index).cloned()?;
        let Some(tool) = self.remote_state.borrow().moonlight.clone() else {
            self.remote_status(tr!("Moonlight n'est pas installé sur ce PC"));
            return None;
        };
        Some((host, tool))
    }

    /// Sélectionne une machine et liste ses jeux publiés (`moonlight list`).
    pub(crate) fn select_remote_host(&self, index: usize) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<RemoteUi>();
        ui.set_selected_host(index as i32);
        ui.set_apps(ModelRc::default());
        let Some((host, tool)) = self.host_and_tool(index) else { return };
        self.remote_busy(true);
        self.remote_status(tr!("Liste des jeux de {name}…", name = host.name));
        std::thread::Builder::new()
            .name("halcyon-remote-list".into())
            .spawn(move || {
                let result = remote_play::list_apps(&tool, &host.address);
                post(move |c| {
                    c.remote_busy(false);
                    let Some(app) = c.app() else { return };
                    let ui = app.global::<RemoteUi>();
                    if ui.get_selected_host() != index as i32 {
                        return;
                    }
                    match result {
                        Ok(list) => {
                            c.remote_status(trn!(list.len(), "{n} jeu publié par {name}", "{n} jeux publiés par {name}", name = host.name));
                            let items: Vec<SharedString> = list.into_iter().map(Into::into).collect();
                            ui.set_apps(ModelRc::new(VecModel::from(items)));
                        }
                        Err(e) => c.remote_status(format!("{} : {e}", host.name)),
                    }
                });
            })
            .ok();
    }

    pub(crate) fn pair_remote(&self, index: usize) {
        let Some((host, tool)) = self.host_and_tool(index) else { return };
        match remote_play::pair(&tool, &host.address) {
            Ok(()) => self.remote_status(tr!("Appairage avec {name} démarré : saisissez le code PIN affiché par Moonlight dans l'interface web de Sunshine de cette machine", name = host.name)),
            Err(e) => self.remote_status(e),
        }
    }

    pub(crate) fn stream_remote(&self, index: usize, app_name: &str) {
        let Some((host, tool)) = self.host_and_tool(index) else { return };
        match remote_play::stream(&tool, &host.address, app_name) {
            Ok(()) => {
                let _ = self.db().log_activity(halcyon_storage::ActivityKind::Launch, None, &tr_later!("Diffusion de « {app_name} » depuis {name}", app_name = app_name, name = host.name), "Moonlight");
                self.remote_status(tr!("Diffusion de « {app_name} » depuis {name}", app_name = app_name, name = host.name));
            }
            Err(e) => self.remote_status(e),
        }
    }

    pub(crate) fn quit_remote(&self, index: usize) {
        let Some((host, tool)) = self.host_and_tool(index) else { return };
        self.remote_status(tr!("Arrêt de la session sur {name}…", name = host.name));
        std::thread::Builder::new()
            .name("halcyon-remote-quit".into())
            .spawn(move || {
                let r = remote_play::quit(&tool, &host.address);
                post(move |c| match r {
                    Ok(()) => c.remote_status(tr!("Session arrêtée sur {name}", name = host.name)),
                    Err(e) => c.remote_status(e),
                });
            })
            .ok();
    }

    pub(crate) fn set_moonlight_path(&self, path: &str) {
        let p = PathBuf::from(path.trim().trim_matches('"'));
        if path.trim().is_empty() {
            self.settings.borrow_mut().moonlight_path = None;
        } else if p.is_file() {
            self.settings.borrow_mut().moonlight_path = Some(p);
        } else {
            self.remote_status(tr!("Fichier introuvable"));
            return;
        }
        self.save_settings();
        self.refresh_remote();
    }

    pub(crate) fn open_sunshine(&self) {
        self.open_url(remote_play::SUNSHINE_WEB);
    }

    /// Le jeu peut-il être publié dans Sunshine depuis sa fiche ?
    pub(crate) fn can_publish(&self) -> bool {
        self.remote_state.borrow().apps_file.is_some()
    }

    /// Ajoute le jeu aux applications de Sunshine : Sunshine lancera Halcyon, qui
    /// applique le profil, les mods et les sauvegardes comme pour un lancement local.
    pub(crate) fn publish_sunshine(self: &Rc<Self>, id: GameId) {
        let Some(apps) = self.remote_state.borrow().apps_file.clone() else { return };
        let Some(g) = self.lib.borrow().get(id).cloned() else { return };
        let exe = match std::env::current_exe() {
            Ok(e) => e,
            Err(e) => return self.toast(tr!("Chemin de Halcyon introuvable : {e}", e = e), true),
        };
        let command = format!("\"{}\" --launch {}", exe.display(), g.id);
        // Sunshine n'affiche que des jaquettes PNG.
        let cover = g.artwork.cover.clone().filter(|p| p.is_file() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")));
        let text = tr!("« {name} » sera ajouté à {apps} (commande : {command}). Une copie de sauvegarde du fichier est créée avant modification. Sunshine doit être redémarré pour voir le jeu.", name = g.name, apps = apps.display(), command = command);
        let name = g.name.clone();
        self.confirm(&tr!("Publier dans Sunshine ?"), &text, &tr!("Publier"), move |c| {
            match remote_play::publish_to_sunshine(&apps, &name, &command, cover.as_deref()) {
                Ok(backup) => {
                    let _ = c.db().log_activity(halcyon_storage::ActivityKind::Settings, Some(id), &tr_later!("« {name} » publié dans Sunshine", name = name), &backup.display().to_string());
                    c.toast(tr!("« {name} » publié dans Sunshine (copie : {backup})", name = name, backup = backup.display()), false);
                }
                Err(e) => c.toast(e, true),
            }
        });
    }
}

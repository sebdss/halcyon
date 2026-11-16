//! Gestionnaire d'extensions : installation (validation puis confirmation),
//! activation explicite, désinstallation, mises à jour et prise en compte des
//! apports (émulateurs, plateformes, sauvegardes, thèmes, dossiers, métadonnées, liens).
//!
//! Les extensions sont déclaratives : aucun code tiers n'est exécuté.

use crate::controller::{post, Controller};
use crate::{LinkRow, PluginRow, PluginsUi};
use halcyon_core::model::{Game, GameSource};
use halcyon_integrations::artwork::Downloader;
use halcyon_integrations::plugins::{self, Manifest};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

impl Controller {
    pub(crate) fn plugins_dir(&self) -> PathBuf {
        self.paths.data_dir.join("plugins")
    }

    fn plugin_staging(&self) -> PathBuf {
        self.paths.cache_dir.join("plugin-staging")
    }

    fn plugins_status(&self, msg: impl Into<SharedString>) {
        if let Some(app) = self.app() {
            app.global::<PluginsUi>().set_status(msg.into());
        }
    }

    /// Lit les extensions installées et retient celles qui sont activées et valides.
    pub(crate) fn load_plugins(&self) {
        let enabled = self.settings.borrow().plugins_enabled.clone();
        let active: Vec<Manifest> = plugins::list(&self.plugins_dir(), VERSION)
            .into_iter()
            .filter(|p| p.usable() && enabled.contains(&p.id()))
            .filter_map(|p| p.manifest)
            .collect();
        let mut saves: HashMap<String, Vec<String>> = HashMap::new();
        for m in &active {
            for loc in &m.provides.save_locations {
                let key = halcyon_core::util::search_key(&loc.game).replace(' ', "");
                saves.entry(key).or_default().extend(loc.paths.iter().cloned());
            }
        }
        *self.plugin_saves.borrow_mut() = saves;
        *self.plugins.borrow_mut() = active;
    }

    /// Recharge tout ce qui dépend des extensions.
    pub(crate) fn apply_plugins(&self) {
        self.load_plugins();
        self.load_registry();
        self.detect_emulators();
        self.refresh_emulators_ui();
        self.apply_appearance();
        self.refresh_plugins_ui();
        self.refresh_detail();
    }

    /// Émulateurs et plateformes des extensions actives (appelé par `load_registry`).
    pub(crate) fn extend_registry(&self, reg: &mut halcyon_integrations::emulators::Registry) {
        for m in self.plugins.borrow().iter() {
            for e in reg.extend(&m.name, &m.provides.emulators, &m.provides.platforms) {
                log::warn!("extension {} : {e}", m.id);
            }
        }
    }

    /// Thèmes des extensions actives : (nom de l'extension, définition).
    pub(crate) fn plugin_themes(&self) -> Vec<(String, serde_json::Value)> {
        self.plugins
            .borrow()
            .iter()
            .flat_map(|m| m.provides.themes.iter().map(move |t| (m.name.clone(), t.clone())))
            .collect()
    }

    /// Dossiers de jeux à analyser déclarés par les extensions (permission « folders »).
    pub(crate) fn plugin_game_folders(&self) -> Vec<PathBuf> {
        let env = self.save_env();
        let mut out = Vec::new();
        for m in self.plugins.borrow().iter().filter(|m| m.permissions.iter().any(|p| p == "folders")) {
            for f in &m.provides.game_folders {
                let f = f.strip_prefix("~/").map(|r| format!("{{home}}/{r}")).unwrap_or_else(|| f.clone());
                if let Some(p) = env.resolve(&f, None).filter(|p| p.is_absolute()) {
                    out.push(p);
                }
            }
        }
        out
    }

    /// Liens des extensions pour la fiche d'un jeu.
    pub(crate) fn plugin_links(&self, g: &Game) -> Vec<LinkRow> {
        let appid = (g.source == GameSource::Steam).then_some(g.source_key.as_str());
        self.plugins
            .borrow()
            .iter()
            .flat_map(|m| m.provides.links.iter())
            .filter(|l| l.url.starts_with("https://"))
            .filter(|l| appid.is_some() || !l.url.contains("{steam_appid}"))
            .map(|l| LinkRow { label: l.label.clone().into(), url: plugins::fill_url(&l.url, &g.name, appid).into() })
            .collect()
    }

    /// Sources de métadonnées actives (extension, source) dont le domaine est autorisé.
    pub(crate) fn plugin_metadata_sources(&self) -> Vec<(Manifest, plugins::MetadataSource)> {
        self.plugins
            .borrow()
            .iter()
            .flat_map(|m| m.provides.metadata.iter().filter(|s| m.allows_url(&s.url)).map(move |s| (m.clone(), s.clone())))
            .collect()
    }

    pub(crate) fn refresh_plugins_ui(&self) {
        let Some(app) = self.app() else { return };
        let enabled = self.settings.borrow().plugins_enabled.clone();
        let updates = self.plugin_updates.borrow();
        let rows: Vec<PluginRow> = plugins::list(&self.plugins_dir(), VERSION)
            .iter()
            .map(|p| {
                let id = p.id();
                let m = p.manifest.clone().unwrap_or_default();
                let perms: Vec<SharedString> = if p.manifest.is_some() { m.permission_summary().into_iter().map(Into::into).collect() } else { vec![] };
                PluginRow {
                    id: id.clone().into(),
                    name: if m.name.is_empty() { id.clone().into() } else { m.name.clone().into() },
                    version: m.version.clone().into(),
                    author: m.author.clone().into(),
                    description: m.description.clone().into(),
                    enabled: p.usable() && enabled.contains(&id),
                    usable: p.usable(),
                    errors: p.errors.join(" · ").into(),
                    warnings: p.warnings.join(" · ").into(),
                    permissions: ModelRc::new(VecModel::from(perms)),
                    update: updates.get(&id).cloned().unwrap_or_default().into(),
                    homepage: if m.homepage.starts_with("https://") { m.homepage.clone().into() } else { SharedString::new() },
                }
            })
            .collect();
        let ui = app.global::<PluginsUi>();
        ui.set_dir(self.plugins_dir().display().to_string().into());
        ui.set_plugins(ModelRc::new(VecModel::from(rows)));
    }

    /// Valide l'extension dans un dossier temporaire puis demande confirmation.
    pub(crate) fn install_plugin(self: &Rc<Self>, path: &str) {
        let src = PathBuf::from(path.trim());
        if !src.exists() {
            self.plugins_status("Chemin introuvable");
            return;
        }
        let staging = self.plugin_staging();
        let manifest = match plugins::stage(&src, &staging, VERSION) {
            Ok(m) => m,
            Err(e) => {
                let _ = std::fs::remove_dir_all(&staging);
                self.plugins_status(format!("Extension refusée : {e}"));
                return;
            }
        };
        let existing = plugins::list(&self.plugins_dir(), VERSION).into_iter().find(|p| p.id() == manifest.id);
        let mut text = manifest.permission_summary().join("\n");
        if let Some(old) = existing.as_ref().and_then(|p| p.manifest.as_ref()) {
            text = format!("Remplace la version {} installée.\n{text}", old.version);
        }
        text.push_str("\n\nL'extension sera installée désactivée : vous pourrez l'activer ensuite.");
        let id = manifest.id.clone();
        let name = manifest.name.clone();
        let perms = manifest.permissions.clone();
        let old_perms = existing.and_then(|p| p.manifest).map(|m| m.permissions);
        self.confirm(&format!("Installer « {} » {} ?", manifest.name, manifest.version), &text, "Installer", move |c| {
            match plugins::install(&c.plugin_staging(), &c.plugins_dir(), &id) {
                Ok(_) => {
                    // Une mise à jour qui demande d'autres autorisations doit être réactivée.
                    if old_perms.as_ref() != Some(&perms) {
                        c.settings.borrow_mut().plugins_enabled.retain(|x| *x != id);
                        c.save_settings();
                    }
                    c.plugin_updates.borrow_mut().remove(&id);
                    c.log_plugin(&format!("Extension « {name} » installée"));
                    halcyon_storage::audit::event("plugin.installed", &format!("{id} permissions=[{}]", perms.join(", ")));
                    c.plugins_status(format!("« {name} » installée"));
                    c.apply_plugins();
                }
                Err(e) => c.plugins_status(format!("Installation impossible : {e}")),
            }
        });
    }

    pub(crate) fn set_plugin_enabled(self: &Rc<Self>, id: &str, on: bool) {
        let id = id.to_string();
        if !on {
            self.settings.borrow_mut().plugins_enabled.retain(|x| *x != id);
            self.save_settings();
            self.log_plugin(&format!("Extension « {id} » désactivée"));
            halcyon_storage::audit::event("plugin.disabled", &id);
            self.apply_plugins();
            return;
        }
        let Some(p) = plugins::list(&self.plugins_dir(), VERSION).into_iter().find(|p| p.id() == id) else { return };
        let Some(m) = p.manifest.clone().filter(|_| p.usable()) else {
            self.plugins_status("Extension inutilisable : corrigez les erreurs indiquées");
            self.refresh_plugins_ui();
            return;
        };
        let text = m.permission_summary().join("\n");
        // L'interrupteur revient à sa position si la confirmation est refusée.
        self.refresh_plugins_ui();
        self.confirm(&format!("Activer « {} » ?", m.name), &text, "Activer", move |c| {
            let mut s = c.settings.borrow_mut();
            if !s.plugins_enabled.contains(&id) {
                s.plugins_enabled.push(id.clone());
            }
            drop(s);
            c.save_settings();
            c.log_plugin(&format!("Extension « {} » activée", m.name));
            halcyon_storage::audit::event("plugin.enabled", &format!("{id} permissions=[{}]", m.permissions.join(", ")));
            c.plugins_status(format!("« {} » activée", m.name));
            c.apply_plugins();
        });
    }

    pub(crate) fn uninstall_plugin(self: &Rc<Self>, id: &str) {
        let id = id.to_string();
        let dir = self.plugins_dir().join(&id);
        if !dir.starts_with(self.plugins_dir()) || !dir.is_dir() {
            return;
        }
        self.confirm(
            &format!("Désinstaller « {id} » ?"),
            "Le dossier de l'extension est supprimé. Vos jeux, sauvegardes et paramètres ne sont pas modifiés ; un thème fourni par l'extension est remplacé par le thème Halcyon.",
            "Désinstaller",
            move |c| match std::fs::remove_dir_all(&dir) {
                Ok(()) => {
                    halcyon_storage::audit::event("plugin.uninstalled", &id);
                    c.settings.borrow_mut().plugins_enabled.retain(|x| *x != id);
                    c.save_settings();
                    c.log_plugin(&format!("Extension « {id} » désinstallée"));
                    c.plugins_status(format!("« {id} » désinstallée"));
                    c.apply_plugins();
                }
                Err(e) => c.plugins_status(format!("Suppression impossible : {e}")),
            },
        );
    }

    pub(crate) fn check_plugin_updates(&self) {
        if !self.net_allowed() {
            self.plugins_status("Hors ligne : vérification impossible");
            return;
        }
        let list: Vec<Manifest> = plugins::list(&self.plugins_dir(), VERSION).into_iter().filter_map(|p| p.manifest).collect();
        if !list.iter().any(|m| !m.update_url.is_empty()) {
            self.plugins_status("Aucune extension n'indique d'adresse de mise à jour");
            return;
        }
        self.plugins_status("Vérification…");
        std::thread::Builder::new()
            .name("halcyon-plugin-updates".into())
            .spawn(move || {
                let dl = Downloader::new();
                let mut found = HashMap::new();
                let mut errors = Vec::new();
                for m in &list {
                    match plugins::check_update(&dl, m) {
                        Ok(Some(v)) => {
                            found.insert(m.id.clone(), v);
                        }
                        Ok(None) => {}
                        Err(e) => errors.push(format!("{} : {e}", m.name)),
                    }
                }
                post(move |c| {
                    let n = found.len();
                    *c.plugin_updates.borrow_mut() = found;
                    let mut msg = if n == 0 { "Toutes les extensions sont à jour".to_string() } else { format!("{n} mise(s) à jour disponible(s) : téléchargez-la depuis la page de l'extension puis installez-la ici") };
                    if !errors.is_empty() {
                        msg.push_str(&format!(" — erreurs : {}", errors.join(" ; ")));
                    }
                    c.plugins_status(msg);
                    c.refresh_plugins_ui();
                });
            })
            .ok();
    }

    pub(crate) fn open_plugins_dir(&self) {
        let dir = self.plugins_dir();
        let _ = std::fs::create_dir_all(&dir);
        self.open_folder(&dir.display().to_string());
    }

    fn log_plugin(&self, title: &str) {
        let _ = self.db().log_activity(halcyon_storage::ActivityKind::Plugin, None, title, "");
    }
}


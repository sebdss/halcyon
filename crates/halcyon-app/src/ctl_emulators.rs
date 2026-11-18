//! Contrôleur — émulateurs : définitions, détection, dossiers de ROM, lancement.

use crate::controller::Controller;
use crate::{EmuUi, EmulatorRow, PlatformChoice, PlatformRow};
use halcyon_core::{Game, GameId, GameSource, LaunchTarget};
use halcyon_integrations::emulators::{self, CustomEmulator, Detected, EmulatorDef, Profile, Registry};
use halcyon_storage::RomFolder;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::path::PathBuf;

impl Controller {
    pub(crate) fn load_registry(&self) {
        let (mut reg, warnings) = Registry::load(&self.paths.data_dir);
        self.extend_registry(&mut reg);
        for w in warnings {
            log::warn!("définition d'émulateur ignorée : {w}");
            self.toast(tr!("Définition ignorée : {w}", w = w), true);
        }
        crate::models::set_platform_names(
            reg.platforms.iter().map(|p| (p.id.clone(), (p.short_or_name().to_string(), p.name.clone()))).collect(),
        );
        *self.registry.borrow_mut() = reg;
    }

    pub(crate) fn detect_emulators(&self) {
        let reg = self.registry.borrow();
        let s = self.settings.borrow();
        let mut found = std::collections::HashMap::new();
        for def in &reg.emulators {
            let forced = s.emulator(&def.id).and_then(|e| e.path.clone());
            if let Some(d) = emulators::detect(def, forced.as_deref()) {
                log::info!("émulateur {} : {} ({})", def.name, d.program.display(), d.via);
                found.insert(def.id.clone(), d);
            }
        }
        *self.detected.borrow_mut() = found;
    }

    /// Émulateur à utiliser : choix du jeu, puis de la plateforme, puis le premier détecté.
    pub(crate) fn emulator_for(&self, game: &Game) -> Option<(EmulatorDef, Detected)> {
        let platform = game.platform.as_deref()?;
        let reg = self.registry.borrow();
        let detected = self.detected.borrow();
        let s = self.settings.borrow();
        let candidates = [game.emulator.clone(), s.platform_emulator.get(platform).cloned()];
        for id in candidates.into_iter().flatten() {
            if let (Some(def), Some(d)) = (reg.emulator(&id), detected.get(&id)) {
                if def.platforms.contains_key(platform) {
                    return Some((def.clone(), d.clone()));
                }
            }
        }
        reg.emulators_for(platform).into_iter().find_map(|def| detected.get(&def.id).map(|d| (def.clone(), d.clone())))
    }

    /// `pad_profile` : profil de manette du profil de lancement (sinon le profil actif).
    pub(crate) fn emulator_target(&self, game: &Game, pad_profile: Option<i64>) -> Result<LaunchTarget, String> {
        let platform = game.platform.clone().unwrap_or_default();
        let rom = game.executable.clone().ok_or(tr!("ROM introuvable"))?;
        let (def, detected) = self.emulator_for(game).ok_or_else(|| {
            let name = self.registry.borrow().platform(&platform).map(|p| p.name.clone()).unwrap_or(platform.clone());
            tr!("Aucun émulateur détecté pour {name} : installez-en un ou indiquez son chemin dans la page Émulateurs", name = name)
        })?;
        let mut profile = self
            .settings
            .borrow()
            .emulator(&def.id)
            .map(|e| Profile { extra_args: e.extra_args.clone(), fullscreen: e.fullscreen })
            .unwrap_or_default();
        // Options de manette propres à cet émulateur, placées avant celles du profil d'émulateur.
        if let Some(pad_args) = self.pad_emulator_args(pad_profile, &def.id) {
            profile.extra_args = format!("{pad_args} {}", profile.extra_args).trim().to_string();
        }
        emulators::build_launch(&def, &detected, &platform, &rom, &profile)
    }

    fn pad_emulator_args(&self, pad_profile: Option<i64>, emulator: &str) -> Option<String> {
        let s = self.settings.borrow();
        let id = pad_profile.unwrap_or(s.active_pad_profile);
        s.pad_profiles
            .iter()
            .find(|p| p.id == id)
            .and_then(|p| p.emulator_args.get(emulator))
            .map(|a| a.trim().to_string())
            .filter(|a| !a.is_empty())
    }

    /// Choix d'émulateur proposé dans la fiche : « Par défaut » puis les émulateurs compatibles.
    pub(crate) fn emulator_options(&self, game: &Game) -> (Vec<SharedString>, usize) {
        let Some(platform) = game.platform.as_deref().filter(|_| game.source == GameSource::Emulated) else {
            return (vec![tr!("Par défaut").into()], 0);
        };
        let default_name = {
            let mut g = game.clone();
            g.emulator = None;
            self.emulator_for(&g).map(|(d, _)| d.name).unwrap_or_else(|| tr!("aucun détecté"))
        };
        let reg = self.registry.borrow();
        let detected = self.detected.borrow();
        let defs = reg.emulators_for(platform);
        let mut options: Vec<SharedString> = vec![tr!("Par défaut ({default_name})", default_name = default_name).into()];
        options.extend(defs.iter().map(|d| {
            if detected.contains_key(&d.id) { d.name.clone().into() } else { tr!("{name} (non détecté)", name = d.name).into() }
        }));
        let index = game.emulator.as_ref().and_then(|e| defs.iter().position(|d| &d.id == e)).map(|i| i + 1).unwrap_or(0);
        (options, index)
    }

    pub(crate) fn set_game_emulator(&self, id: GameId, index: usize) {
        let Some(game) = self.lib.borrow().get(id).cloned() else { return };
        let Some(platform) = game.platform.clone() else { return };
        let choice = if index == 0 {
            None
        } else {
            self.registry.borrow().emulators_for(&platform).get(index - 1).map(|d| d.id.clone())
        };
        if let Err(e) = self.db().set_game_emulator(id, choice.as_deref()) {
            self.toast(tr!("Erreur : {e}", e = e), true);
            return;
        }
        if let Some(g) = self.lib.borrow_mut().get_mut(id) {
            g.emulator = choice;
        }
        self.refresh_detail();
    }

    pub(crate) fn refresh_emulators_ui(&self) {
        let Some(app) = self.app() else { return };
        let reg = self.registry.borrow();
        let detected = self.detected.borrow();
        let s = self.settings.borrow();
        let lib = self.lib.borrow();

        let rows: Vec<EmulatorRow> = reg
            .emulators
            .iter()
            .map(|def| {
                let d = detected.get(&def.id);
                let profile = s.emulator(&def.id);
                let platforms: Vec<String> = def
                    .platforms
                    .keys()
                    .filter_map(|p| reg.platform(p).map(|p| p.short_or_name().to_string()))
                    .collect();
                EmulatorRow {
                    id: def.id.as_str().into(),
                    name: def.name.as_str().into(),
                    detected: d.is_some(),
                    path: d.map(|d| d.program.display().to_string()).unwrap_or_default().into(),
                    via: d.map(|d| halcyon_i18n::t(d.via)).unwrap_or_default().into(),
                    platforms: platforms.join(" · ").into(),
                    extra_args: profile.map(|p| p.extra_args.clone()).unwrap_or_default().into(),
                    fullscreen: profile.is_some_and(|p| p.fullscreen),
                    path_override: profile.and_then(|p| p.path.as_ref()).map(|p| p.display().to_string()).unwrap_or_default().into(),
                    custom: def.id.starts_with(emulators::CUSTOM_PREFIX),
                }
            })
            .collect();

        let platforms: Vec<PlatformRow> = reg
            .platforms
            .iter()
            .filter(|p| s.rom_folders.iter().any(|f| f.platform == p.id))
            .map(|p| {
                let defs = reg.emulators_for(&p.id);
                let names: Vec<SharedString> = defs
                    .iter()
                    .map(|d| if detected.contains_key(&d.id) { d.name.clone().into() } else { tr!("{name} (non détecté)", name = d.name).into() })
                    .collect();
                let selected = s
                    .platform_emulator
                    .get(&p.id)
                    .and_then(|id| defs.iter().position(|d| &d.id == id))
                    .or_else(|| defs.iter().position(|d| detected.contains_key(&d.id)))
                    .unwrap_or(0);
                let folders: Vec<SharedString> =
                    s.rom_folders.iter().filter(|f| f.platform == p.id).map(|f| f.path.display().to_string().into()).collect();
                PlatformRow {
                    id: p.id.as_str().into(),
                    name: p.name.as_str().into(),
                    short: p.short_or_name().into(),
                    emulators: ModelRc::new(VecModel::from(if names.is_empty() { vec![SharedString::from(tr!("Aucun"))] } else { names })),
                    selected: selected as i32,
                    folders: ModelRc::new(VecModel::from(folders)),
                    games: lib.games.iter().filter(|g| g.platform.as_deref() == Some(&p.id) && g.installed).count() as i32,
                    ra: p.ra_console.is_some(),
                }
            })
            .collect();

        let labels: Vec<SharedString> = reg.platforms.iter().map(|p| p.name.as_str().into()).collect();
        let emu = app.global::<EmuUi>();
        emu.set_detected_count(detected.len() as i32);
        emu.set_rom_count(lib.games.iter().filter(|g| g.source == GameSource::Emulated && g.installed).count() as i32);
        emu.set_emulators(ModelRc::new(VecModel::from(rows)));
        emu.set_platforms(ModelRc::new(VecModel::from(platforms)));
        emu.set_platform_labels(ModelRc::new(VecModel::from(labels)));
        emu.set_defs_dir(self.paths.data_dir.join("emulators").display().to_string().into());
    }

    // ------------------------------------------------------------ émulateurs ajoutés par l'utilisateur

    /// Plateformes du formulaire, cochées selon `checked`.
    fn form_platforms(&self, checked: &[String]) -> Vec<PlatformChoice> {
        let reg = self.registry.borrow();
        let mut v: Vec<PlatformChoice> = reg
            .platforms
            .iter()
            .map(|p| PlatformChoice { id: p.id.as_str().into(), name: p.name.as_str().into(), checked: checked.contains(&p.id) })
            .collect();
        v.sort_by_key(|a| a.name.to_lowercase());
        v
    }

    /// Ouvre le formulaire : `id` vide pour un nouvel émulateur.
    pub(crate) fn open_emulator_form(&self, id: &str) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<EmuUi>();
        let e = if id.is_empty() {
            CustomEmulator { args: "{rom}".into(), ..Default::default() }
        } else {
            match emulators::read_custom(&self.paths.data_dir, id) {
                Some(e) => e,
                None => return self.toast(tr!("Définition introuvable ou illisible"), true),
            }
        };
        ui.set_form_id(id.into());
        ui.set_form_name(e.name.as_str().into());
        ui.set_form_exe(if e.executable.as_os_str().is_empty() { String::new() } else { e.executable.display().to_string() }.into());
        ui.set_form_args(e.args.as_str().into());
        ui.set_form_fs_args(e.fullscreen_args.as_str().into());
        ui.set_form_platforms(ModelRc::new(VecModel::from(self.form_platforms(&e.platforms))));
        ui.set_form_error("".into());
        ui.set_form_open(true);
    }

    fn checked_platforms(ui: &EmuUi) -> Vec<String> {
        use slint::Model;
        ui.get_form_platforms().iter().filter(|p| p.checked).map(|p| p.id.to_string()).collect()
    }

    pub(crate) fn toggle_form_platform(&self, index: usize, on: bool) {
        use slint::Model;
        let Some(app) = self.app() else { return };
        let model = app.global::<EmuUi>().get_form_platforms();
        if let Some(mut row) = model.row_data(index) {
            row.checked = on;
            model.set_row_data(index, row);
        }
    }

    pub(crate) fn save_emulator_form(&self) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<EmuUi>();
        let e = CustomEmulator {
            name: ui.get_form_name().trim().to_string(),
            executable: PathBuf::from(ui.get_form_exe().trim().trim_matches('"')),
            platforms: Self::checked_platforms(&ui),
            args: ui.get_form_args().trim().to_string(),
            fullscreen_args: ui.get_form_fs_args().trim().to_string(),
        };
        if let Err(msg) = emulators::validate_custom(&e, &self.registry.borrow()) {
            ui.set_form_error(capitalize(&msg).into());
            return;
        }
        let id = ui.get_form_id().to_string();
        match emulators::save_custom(&self.paths.data_dir, (!id.is_empty()).then_some(id.as_str()), &e) {
            Ok(new_id) => {
                log::info!("émulateur enregistré : {} ({new_id})", e.name);
                ui.set_form_open(false);
                self.load_registry();
                self.after_emulator_change();
                let ok = self.detected.borrow().contains_key(&new_id);
                self.toast(
                    if ok { tr!("{name} ajouté : importez vos ROM pour ses plateformes", name = e.name) } else { tr!("{name} enregistré, mais son exécutable n'est pas trouvé", name = e.name) },
                    !ok,
                );
            }
            Err(msg) => ui.set_form_error(capitalize(&msg).into()),
        }
    }

    pub(crate) fn delete_custom_emulator(&self, id: &str) {
        let name = self.registry.borrow().emulator(id).map(|d| d.name.clone()).unwrap_or_else(|| id.to_string());
        let id = id.to_string();
        self.confirm(
            &tr!("Supprimer {name} ?", name = name),
            &tr!("Sa définition est retirée de Halcyon. L'émulateur lui-même, ses fichiers et vos jeux ne sont pas touchés."),
            &tr!("Supprimer"),
            move |c| match emulators::delete_custom(&c.paths.data_dir, &id) {
                Ok(()) => {
                    {
                        let mut s = c.settings.borrow_mut();
                        s.platform_emulator.retain(|_, v| v != &id);
                    }
                    c.load_registry();
                    c.after_emulator_change();
                    c.toast(tr!("{name} supprimé", name = name), false);
                }
                Err(e) => c.toast(e, true),
            },
        );
    }

    pub(crate) fn add_custom_platform(&self, name: &str, exts: &str) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<EmuUi>();
        match emulators::save_custom_platform(&self.paths.data_dir, name, exts) {
            Ok(id) => {
                let mut checked = Self::checked_platforms(&ui);
                checked.push(id);
                self.load_registry();
                ui.set_form_platforms(ModelRc::new(VecModel::from(self.form_platforms(&checked))));
                ui.set_form_error("".into());
                self.refresh_emulators_ui();
            }
            Err(msg) => ui.set_form_error(capitalize(&msg).into()),
        }
    }

    pub(crate) fn redetect_emulators(&self) {
        self.load_registry();
        self.detect_emulators();
        self.refresh_emulators_ui();
        self.refresh_detail();
        let n = self.detected.borrow().len();
        self.toast(trn!(n, "{n} émulateur détecté", "{n} émulateurs détectés"), false);
    }

    fn after_emulator_change(&self) {
        self.save_settings();
        self.detect_emulators();
        self.refresh_emulators_ui();
        self.refresh_detail();
    }

    pub(crate) fn set_emulator_path(&self, id: &str, path: &str) {
        let path = Some(PathBuf::from(path.trim().trim_matches('"'))).filter(|p| !p.as_os_str().is_empty());
        if let Some(p) = &path {
            if !p.is_file() {
                self.toast(tr!("Exécutable introuvable : {p}", p = p.display()), true);
                return;
            }
        }
        self.settings.borrow_mut().emulator_mut(id).path = path;
        self.after_emulator_change();
        let ok = self.detected.borrow().contains_key(id);
        self.toast(if ok { tr!("Émulateur configuré") } else { tr!("Émulateur toujours introuvable") }, !ok);
    }

    pub(crate) fn set_emulator_args(&self, id: &str, args: &str) {
        self.settings.borrow_mut().emulator_mut(id).extra_args = args.trim().to_string();
        self.after_emulator_change();
        self.toast(tr!("Arguments enregistrés"), false);
    }

    pub(crate) fn set_emulator_fullscreen(&self, id: &str, value: bool) {
        self.settings.borrow_mut().emulator_mut(id).fullscreen = value;
        self.after_emulator_change();
    }

    pub(crate) fn set_platform_default(&self, platform: &str, index: usize) {
        let id = self.registry.borrow().emulators_for(platform).get(index).map(|d| d.id.clone());
        if let Some(id) = id {
            self.settings.borrow_mut().platform_emulator.insert(platform.to_string(), id);
            self.after_emulator_change();
        }
    }

    pub(crate) fn add_rom_folder(&self, platform_index: usize, path: &str) {
        let path = PathBuf::from(path.trim().trim_matches('"'));
        if !path.is_dir() {
            self.toast(tr!("Dossier introuvable : {path}", path = path.display()), true);
            return;
        }
        let Some(platform) = self.registry.borrow().platforms.get(platform_index).map(|p| p.id.clone()) else { return };
        let key = halcyon_core::util::path_key(&path);
        {
            let mut s = self.settings.borrow_mut();
            if s.rom_folders.iter().any(|f| f.platform == platform && halcyon_core::util::path_key(&f.path) == key) {
                drop(s);
                self.toast(tr!("Ce dossier est déjà associé à cette plateforme"), true);
                return;
            }
            s.rom_folders.push(RomFolder { platform, path });
        }
        self.save_settings();
        self.refresh_emulators_ui();
        self.start_scan();
    }

    pub(crate) fn remove_rom_folder(&self, platform: &str, index: usize) {
        {
            let mut s = self.settings.borrow_mut();
            let positions: Vec<usize> =
                s.rom_folders.iter().enumerate().filter(|(_, f)| f.platform == platform).map(|(i, _)| i).collect();
            let Some(&pos) = positions.get(index) else { return };
            s.rom_folders.remove(pos);
        }
        self.save_settings();
        self.refresh_emulators_ui();
        self.start_scan();
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

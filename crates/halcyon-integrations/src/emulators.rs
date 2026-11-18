//! Émulateurs, décrits de façon déclarative.
//!
//! Les définitions intégrées (`data/emulators.json`, `data/platforms.json`) peuvent
//! être complétées ou remplacées par des fichiers JSON placés dans
//! `<données>/emulators/` et `<données>/platforms/` : ajouter un émulateur ne
//! demande aucune modification du code.
//!
//! Gabarits d'arguments : `{rom}`, `{rom_dir}`, `{rom_name}`, `{core_dir}`,
//! `{lib_ext}` (`.dll` / `.so` / `.dylib`), `{emu_dir}`.

use halcyon_core::launch_args::split_args;
use halcyon_core::LaunchTarget;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const BUILTIN_PLATFORMS: &str = include_str!("../data/platforms.json");
const BUILTIN_EMULATORS: &str = include_str!("../data/emulators.json");

/// Valeur dépendant du système d'exploitation.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct PerOs<T: Default> {
    #[serde(default)]
    pub windows: T,
    #[serde(default)]
    pub linux: T,
    #[serde(default)]
    pub macos: T,
}

impl<T: Default> PerOs<T> {
    pub fn current(&self) -> &T {
        if cfg!(windows) {
            &self.windows
        } else if cfg!(target_os = "macos") {
            &self.macos
        } else {
            &self.linux
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct PlatformDef {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub short: String,
    pub extensions: Vec<String>,
    /// Identifiant de console RetroAchievements.
    #[serde(default)]
    pub ra_console: Option<u32>,
    /// Méthode d'empreinte RetroAchievements (voir `rahash`).
    #[serde(default = "default_hash")]
    pub ra_hash: String,
    /// Jeux distribués sous forme de dossier (ex. « ps3 » : `PS3_GAME/USRDIR/EBOOT.BIN`).
    #[serde(default)]
    pub folder_game: Option<String>,
    /// Nom du système dans libretro-thumbnails (visuels sans clé), ex. « Sony - PlayStation ».
    #[serde(default)]
    pub libretro: Option<String>,
}

fn default_hash() -> String {
    "none".into()
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct PlatformLaunch {
    pub args: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct EmulatorDef {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub executables: PerOs<Vec<String>>,
    #[serde(default)]
    pub search_paths: PerOs<Vec<String>>,
    /// Identifiant Flatpak (Linux).
    #[serde(default)]
    pub flatpak: Option<String>,
    /// Dossiers des cœurs (RetroArch).
    #[serde(default)]
    pub cores_dirs: PerOs<Vec<String>>,
    #[serde(default)]
    pub fullscreen_args: Vec<String>,
    pub platforms: BTreeMap<String, PlatformLaunch>,
}

/// Ensemble des définitions connues.
#[derive(Clone, Debug, Default)]
pub struct Registry {
    pub platforms: Vec<PlatformDef>,
    pub emulators: Vec<EmulatorDef>,
}

impl Registry {
    /// Définitions intégrées uniquement.
    pub fn builtin() -> Self {
        Self {
            platforms: serde_json::from_str(BUILTIN_PLATFORMS).expect("platforms.json intégré valide"),
            emulators: serde_json::from_str(BUILTIN_EMULATORS).expect("emulators.json intégré valide"),
        }
    }

    /// Définitions intégrées + fichiers de l'utilisateur. Renvoie aussi les erreurs de lecture.
    pub fn load(data_dir: &Path) -> (Self, Vec<String>) {
        let mut reg = Self::builtin();
        let mut warnings = Vec::new();
        for (dir, is_emu) in [(data_dir.join("platforms"), false), (data_dir.join("emulators"), true)] {
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for e in entries.flatten() {
                let p = e.path();
                if p.extension().and_then(|x| x.to_str()) != Some("json") {
                    continue;
                }
                let text = match std::fs::read_to_string(&p) {
                    Ok(t) => t,
                    Err(err) => {
                        warnings.push(format!("{} : {err}", p.display()));
                        continue;
                    }
                };
                let result = if is_emu {
                    parse_one_or_many::<EmulatorDef>(&text).map(|defs| {
                        for d in defs {
                            reg.emulators.retain(|x| x.id != d.id);
                            reg.emulators.push(d);
                        }
                    })
                } else {
                    parse_one_or_many::<PlatformDef>(&text).map(|defs| {
                        for d in defs {
                            reg.platforms.retain(|x| x.id != d.id);
                            reg.platforms.push(d);
                        }
                    })
                };
                if let Err(err) = result {
                    warnings.push(format!("{} : {err}", p.display()));
                }
            }
        }
        (reg, warnings)
    }

    /// Ajoute des définitions fournies par une extension (`source` : nom de l'extension).
    /// Une définition intégrée de même identifiant n'est jamais remplacée.
    pub fn extend(&mut self, source: &str, emulators: &[serde_json::Value], platforms: &[serde_json::Value]) -> Vec<String> {
        let mut errors = Vec::new();
        let builtin = Self::builtin();
        for v in platforms {
            match serde_json::from_value::<PlatformDef>(v.clone()) {
                Ok(d) if builtin.platform(&d.id).is_some() => {
                    errors.push(tr!("{source} : la plateforme intégrée « {id} » ne peut pas être remplacée", source = source, id = d.id))
                }
                Ok(d) => {
                    self.platforms.retain(|x| x.id != d.id);
                    self.platforms.push(d);
                }
                Err(e) => errors.push(tr!("{source} : plateforme invalide ({e})", source = source, e = e)),
            }
        }
        for v in emulators {
            match serde_json::from_value::<EmulatorDef>(v.clone()) {
                Ok(d) if builtin.emulator(&d.id).is_some() => {
                    errors.push(tr!("{source} : l'émulateur intégré « {id} » ne peut pas être remplacé", source = source, id = d.id))
                }
                Ok(d) => {
                    self.emulators.retain(|x| x.id != d.id);
                    self.emulators.push(d);
                }
                Err(e) => errors.push(tr!("{source} : émulateur invalide ({e})", source = source, e = e)),
            }
        }
        errors
    }

    pub fn platform(&self, id: &str) -> Option<&PlatformDef> {
        self.platforms.iter().find(|p| p.id == id)
    }

    pub fn emulator(&self, id: &str) -> Option<&EmulatorDef> {
        self.emulators.iter().find(|e| e.id == id)
    }

    /// Émulateurs capables de lancer une plateforme.
    pub fn emulators_for(&self, platform: &str) -> Vec<&EmulatorDef> {
        self.emulators.iter().filter(|e| e.platforms.contains_key(platform)).collect()
    }
}

fn parse_one_or_many<T: for<'de> Deserialize<'de>>(text: &str) -> Result<Vec<T>, serde_json::Error> {
    serde_json::from_str::<Vec<T>>(text).or_else(|_| serde_json::from_str::<T>(text).map(|one| vec![one]))
}

/// Remplace `%VAR%` (Windows) et `~` (Unix) dans un chemin de recherche.
pub fn expand_path(raw: &str) -> PathBuf {
    let mut out = String::new();
    let mut rest = raw;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) => {
                let var = &after[..end];
                out.push_str(&std::env::var(var).unwrap_or_else(|_| format!("%{var}%")));
                rest = &after[end + 1..];
            }
            None => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    if let Some(stripped) = out.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
            return PathBuf::from(home).join(stripped);
        }
    }
    PathBuf::from(out)
}

/// Émulateur trouvé sur la machine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Detected {
    pub program: PathBuf,
    /// Comment il a été trouvé : « manuel », « PATH », « Flatpak », « dossier ».
    pub via: &'static str,
}

/// Cherche l'exécutable d'un émulateur : chemin forcé, dossiers usuels, PATH, Flatpak.
pub fn detect(def: &EmulatorDef, forced: Option<&Path>) -> Option<Detected> {
    if let Some(p) = forced {
        return p.is_file().then(|| Detected { program: p.to_path_buf(), via: tr_noop!("manuel") });
    }
    let names = def.executables.current();
    for dir in def.search_paths.current() {
        let dir = expand_path(dir);
        for n in names {
            let p = dir.join(n);
            if p.is_file() {
                return Some(Detected { program: p, via: tr_noop!("dossier") });
            }
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            for n in names {
                let p = dir.join(n);
                if p.is_file() {
                    return Some(Detected { program: p, via: "PATH" });
                }
            }
        }
    }
    if cfg!(target_os = "linux") {
        if let Some(app) = &def.flatpak {
            let mut roots = vec![PathBuf::from("/var/lib/flatpak/exports/bin")];
            roots.push(expand_path("~/.local/share/flatpak/exports/bin"));
            for r in roots {
                let p = r.join(app);
                if p.is_file() {
                    return Some(Detected { program: p, via: "Flatpak" });
                }
            }
        }
    }
    None
}

/// Dossier des cœurs (RetroArch) : le premier existant, sinon le premier candidat.
pub fn cores_dir(def: &EmulatorDef, program: &Path) -> Option<PathBuf> {
    let emu_dir = program.parent().map(|p| p.display().to_string()).unwrap_or_default();
    let candidates: Vec<PathBuf> =
        def.cores_dirs.current().iter().map(|c| expand_path(&c.replace("{emu_dir}", &emu_dir))).collect();
    candidates.iter().find(|c| c.is_dir()).cloned().or_else(|| candidates.first().cloned())
}

fn lib_ext() -> &'static str {
    if cfg!(windows) {
        ".dll"
    } else if cfg!(target_os = "macos") {
        ".dylib"
    } else {
        ".so"
    }
}

/// Options de profil utilisées pour construire la ligne de commande.
#[derive(Clone, Debug, Default)]
pub struct Profile {
    pub extra_args: String,
    pub fullscreen: bool,
}

/// Construit la commande de lancement d'une ROM. Vérifie la présence du cœur
/// RetroArch le cas échéant, pour un message clair plutôt qu'un échec silencieux.
pub fn build_launch(
    def: &EmulatorDef,
    detected: &Detected,
    platform: &str,
    rom: &Path,
    profile: &Profile,
) -> Result<LaunchTarget, String> {
    let launch = def.platforms.get(platform).ok_or_else(|| tr!("{name} ne prend pas en charge cette plateforme", name = def.name))?;
    if !rom.exists() {
        return Err(tr!("ROM introuvable : {rom}", rom = rom.display()));
    }
    let core_dir = cores_dir(def, &detected.program);
    let vars: Vec<(&str, String)> = vec![
        ("{rom}", rom.display().to_string()),
        ("{rom_dir}", rom.parent().map(|p| p.display().to_string()).unwrap_or_default()),
        ("{rom_name}", rom.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()),
        ("{core_dir}", core_dir.as_ref().map(|p| p.display().to_string()).unwrap_or_default()),
        ("{lib_ext}", lib_ext().to_string()),
        ("{emu_dir}", detected.program.parent().map(|p| p.display().to_string()).unwrap_or_default()),
    ];
    let render = |t: &str| vars.iter().fold(t.to_string(), |acc, (k, v)| acc.replace(k, v));

    let mut args: Vec<String> = split_args(&profile.extra_args);
    if profile.fullscreen {
        args.extend(def.fullscreen_args.iter().map(|a| render(a)));
    }
    for t in &launch.args {
        let value = render(t);
        if t.contains("{core_dir}") && !Path::new(&value).is_file() {
            return Err(tr!("cœur introuvable : {value} — installez-le depuis RetroArch (Menu principal › Charger un cœur › Télécharger un cœur)", value = value));
        }
        args.push(value);
    }
    Ok(LaunchTarget::Process { program: detected.program.clone(), args, working_dir: detected.program.parent().map(Path::to_path_buf) })
}

// ---------------------------------------------------------------- émulateurs ajoutés par l'utilisateur

/// Émulateur décrit dans l'interface (enregistré en JSON dans `<données>/emulators/`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CustomEmulator {
    pub name: String,
    pub executable: PathBuf,
    /// Plateformes prises en charge.
    pub platforms: Vec<String>,
    /// Arguments de lancement, avec `{rom}` (et les autres variables).
    pub args: String,
    /// Arguments ajoutés quand « plein écran » est activé.
    pub fullscreen_args: String,
}

/// Préfixe des identifiants des émulateurs et plateformes créés dans l'interface : ils ne
/// peuvent pas remplacer une définition intégrée.
pub const CUSTOM_PREFIX: &str = "perso-";

/// Identifiant à partir d'un nom : « Xemu (dev) » → « perso-xemu-dev ».
pub fn custom_id(name: &str) -> String {
    let slug: String = halcyon_core::util::search_key(name).split_whitespace().collect::<Vec<_>>().join("-");
    format!("{CUSTOM_PREFIX}{}", if slug.is_empty() { "emulateur".into() } else { slug })
}

/// Vérifie la saisie ; renvoie le message à afficher en cas de problème.
pub fn validate_custom(e: &CustomEmulator, reg: &Registry) -> Result<(), String> {
    if e.name.trim().is_empty() {
        return Err(tr!("indiquez le nom de l'émulateur").into());
    }
    if !e.executable.is_file() {
        return Err(tr!("exécutable introuvable : {executable}", executable = e.executable.display()));
    }
    if e.platforms.is_empty() {
        return Err(tr!("choisissez au moins une plateforme").into());
    }
    if let Some(p) = e.platforms.iter().find(|p| reg.platform(p).is_none()) {
        return Err(tr!("plateforme inconnue : {p}", p = p));
    }
    if !e.args.contains("{rom") {
        return Err(tr!("les arguments doivent contenir {rom} (ou {rom_dir}, {rom_name}) pour transmettre le jeu"));
    }
    Ok(())
}

/// Enregistre (ou remplace) un émulateur de l'utilisateur ; renvoie son identifiant.
/// `id` : identifiant existant à modifier, sinon un nouveau est créé à partir du nom.
pub fn save_custom(data_dir: &Path, id: Option<&str>, e: &CustomEmulator) -> Result<String, String> {
    let id = match id {
        Some(id) if id.starts_with(CUSTOM_PREFIX) => id.to_string(),
        Some(id) => return Err(tr!("« {id} » est un émulateur intégré : il ne peut pas être modifié ici", id = id)),
        None => {
            let base = custom_id(&e.name);
            let mut id = base.clone();
            let mut n = 2;
            while data_dir.join("emulators").join(format!("{id}.json")).exists() {
                id = format!("{base}-{n}");
                n += 1;
            }
            id
        }
    };
    let exe_name = e.executable.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let exe_dir = e.executable.parent().map(|d| d.display().to_string()).unwrap_or_default();
    let os = if cfg!(windows) { "windows" } else if cfg!(target_os = "macos") { "macos" } else { "linux" };
    let args = split_args(&e.args);
    let platforms: serde_json::Map<String, serde_json::Value> =
        e.platforms.iter().map(|p| (p.clone(), serde_json::json!({ "args": args }))).collect();
    let def = serde_json::json!({
        "id": id,
        "name": e.name.trim(),
        "executables": { os: [exe_name] },
        "search_paths": { os: [exe_dir] },
        "fullscreen_args": split_args(&e.fullscreen_args),
        "platforms": platforms,
        "added_by": "halcyon",
    });
    // Vérifie que le fichier se relit comme une définition valide avant de l'écrire.
    serde_json::from_value::<EmulatorDef>(def.clone()).map_err(|err| tr!("définition invalide : {err}", err = err))?;
    let dir = data_dir.join("emulators");
    std::fs::create_dir_all(&dir).map_err(|err| format!("{} : {err}", dir.display()))?;
    let path = dir.join(format!("{id}.json"));
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(&def).unwrap_or_default()).map_err(|err| err.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|err| err.to_string())?;
    Ok(id)
}

/// Relit un émulateur de l'utilisateur pour le modifier.
pub fn read_custom(data_dir: &Path, id: &str) -> Option<CustomEmulator> {
    if !id.starts_with(CUSTOM_PREFIX) {
        return None;
    }
    let text = std::fs::read_to_string(data_dir.join("emulators").join(format!("{id}.json"))).ok()?;
    let def: EmulatorDef = serde_json::from_str(&text).ok()?;
    let exe_name = def.executables.current().first().cloned().unwrap_or_default();
    let exe_dir = def.search_paths.current().first().cloned().unwrap_or_default();
    let join = |v: &[String]| v.iter().map(|a| if a.contains(' ') { format!("\"{a}\"") } else { a.clone() }).collect::<Vec<_>>().join(" ");
    Some(CustomEmulator {
        name: def.name.clone(),
        executable: PathBuf::from(exe_dir).join(exe_name),
        platforms: def.platforms.keys().cloned().collect(),
        args: def.platforms.values().next().map(|l| join(&l.args)).unwrap_or_default(),
        fullscreen_args: join(&def.fullscreen_args),
    })
}

/// Supprime un émulateur de l'utilisateur (jamais une définition intégrée ni le fichier
/// d'une extension).
pub fn delete_custom(data_dir: &Path, id: &str) -> Result<(), String> {
    if !id.starts_with(CUSTOM_PREFIX) {
        return Err(tr!("seuls les émulateurs ajoutés dans Halcyon peuvent être supprimés").into());
    }
    let path = data_dir.join("emulators").join(format!("{id}.json"));
    std::fs::remove_file(&path).map_err(|err| format!("{} : {err}", path.display()))
}

/// Ajoute une plateforme (extensions de ROM) ; renvoie son identifiant.
pub fn save_custom_platform(data_dir: &Path, name: &str, extensions: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(tr!("indiquez le nom de la plateforme").into());
    }
    let exts: Vec<String> = extensions
        .split([',', ' ', ';'])
        .map(|x| x.trim().trim_start_matches("*.").trim_start_matches('.').to_lowercase())
        .filter(|x| !x.is_empty() && x.chars().all(|c| c.is_ascii_alphanumeric()))
        .collect();
    if exts.is_empty() {
        return Err(tr!("indiquez au moins une extension de fichier (ex. iso, chd)").into());
    }
    let id = custom_id(name);
    let dir = data_dir.join("platforms");
    std::fs::create_dir_all(&dir).map_err(|err| format!("{} : {err}", dir.display()))?;
    let def = serde_json::json!({ "id": id, "name": name, "extensions": exts });
    serde_json::from_value::<PlatformDef>(def.clone()).map_err(|err| tr!("plateforme invalide : {err}", err = err))?;
    std::fs::write(dir.join(format!("{id}.json")), serde_json::to_string_pretty(&def).unwrap_or_default()).map_err(|err| err.to_string())?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_definitions_are_consistent() {
        let reg = Registry::builtin();
        assert!(reg.platforms.len() >= 20);
        for e in &reg.emulators {
            for p in e.platforms.keys() {
                assert!(reg.platform(p).is_some(), "{} référence une plateforme inconnue : {p}", e.id);
            }
        }
        for id in ["retroarch", "rpcs3", "pcsx2", "dolphin", "cemu", "ryujinx"] {
            assert!(reg.emulator(id).is_some(), "{id} manquant");
        }
        assert!(reg.emulators_for("snes").iter().any(|e| e.id == "retroarch"));
    }

    #[test]
    fn user_definitions_extend_and_override() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("emulators")).unwrap();
        std::fs::write(
            tmp.path().join("emulators/xemu.json"),
            r#"{"id":"xemu","name":"xemu","executables":{"linux":["xemu"],"windows":["xemu.exe"]},
                "platforms":{"xbox":{"args":["-dvd_path","{rom}"]}}}"#,
        )
        .unwrap();
        std::fs::create_dir_all(tmp.path().join("platforms")).unwrap();
        std::fs::write(tmp.path().join("platforms/xbox.json"), r#"{"id":"xbox","name":"Xbox","extensions":["iso"]}"#).unwrap();
        std::fs::write(tmp.path().join("emulators/broken.json"), "{").unwrap();
        let (reg, warnings) = Registry::load(tmp.path());
        assert_eq!(warnings.len(), 1);
        assert_eq!(reg.emulators_for("xbox")[0].id, "xemu");
        assert_eq!(reg.platform("xbox").unwrap().name, "Xbox");
    }

    #[test]
    fn builds_command_lines_and_checks_cores() {
        let tmp = tempfile::tempdir().unwrap();
        let rom = tmp.path().join("Super Game (USA).sfc");
        std::fs::write(&rom, b"rom").unwrap();
        let program = tmp.path().join("emu").join("dolphin");
        let def = EmulatorDef {
            id: "x".into(),
            name: "X".into(),
            executables: PerOs::default(),
            search_paths: PerOs::default(),
            flatpak: None,
            cores_dirs: PerOs { windows: vec!["{emu_dir}/cores".into()], linux: vec!["{emu_dir}/cores".into()], macos: vec![] },
            fullscreen_args: vec!["-f".into()],
            platforms: [
                ("snes".to_string(), PlatformLaunch { args: vec!["-L".into(), "{core_dir}/snes9x_libretro{lib_ext}".into(), "{rom}".into()] }),
                ("gc".to_string(), PlatformLaunch { args: vec!["-b".into(), "-e".into(), "{rom}".into()] }),
            ]
            .into_iter()
            .collect(),
        };
        let det = Detected { program: program.clone(), via: tr_noop!("manuel") };
        let profile = Profile { extra_args: "--verbose \"a b\"".into(), fullscreen: true };

        match build_launch(&def, &det, "gc", &rom, &profile).unwrap() {
            LaunchTarget::Process { program: p, args, .. } => {
                assert_eq!(p, program);
                assert_eq!(args, vec!["--verbose", "a b", "-f", "-b", "-e", rom.to_str().unwrap()]);
            }
            other => panic!("{other:?}"),
        }
        let err = build_launch(&def, &det, "snes", &rom, &Profile::default()).unwrap_err();
        assert!(err.contains("cœur introuvable"), "{err}");

        std::fs::create_dir_all(tmp.path().join("emu/cores")).unwrap();
        std::fs::write(tmp.path().join(format!("emu/cores/snes9x_libretro{}", lib_ext())), b"").unwrap();
        assert!(build_launch(&def, &det, "snes", &rom, &Profile::default()).is_ok());
        assert!(build_launch(&def, &det, "n64", &rom, &Profile::default()).is_err());
    }

    #[test]
    fn expands_environment_paths() {
        std::env::set_var("HALCYON_TEST_DIR", "/opt/test");
        assert_eq!(expand_path("%HALCYON_TEST_DIR%/RetroArch"), PathBuf::from("/opt/test/RetroArch"));
        assert_eq!(expand_path("50%"), PathBuf::from("50%"));
    }

    #[test]
    fn detects_forced_and_search_path_executables() {
        let tmp = tempfile::tempdir().unwrap();
        let exe = tmp.path().join(if cfg!(windows) { "emu.exe" } else { "emu" });
        std::fs::write(&exe, b"").unwrap();
        let mut def: EmulatorDef = serde_json::from_str(r#"{"id":"e","name":"E","platforms":{}}"#).unwrap();
        assert!(detect(&def, None).is_none());
        assert_eq!(detect(&def, Some(&exe)).unwrap().via, "manuel");
        let name = exe.file_name().unwrap().to_string_lossy().into_owned();
        def.executables = PerOs { windows: vec![name.clone()], linux: vec![name.clone()], macos: vec![name] };
        def.search_paths = PerOs {
            windows: vec![tmp.path().display().to_string()],
            linux: vec![tmp.path().display().to_string()],
            macos: vec![],
        };
        assert_eq!(detect(&def, None).unwrap(), Detected { program: exe, via: "dossier" });
    }

    #[test]
    fn plugin_example_from_docs_is_accepted() {
        let mut reg = Registry::builtin();
        let platforms = vec![serde_json::json!({ "id": "vectrex", "name": "Vectrex", "short": "VEC", "extensions": ["vec", "bin"] })];
        let emulators = vec![serde_json::json!({
            "id": "vecx", "name": "Vecx",
            "executables": { "windows": ["vecx.exe"], "linux": ["vecx"] },
            "search_paths": { "windows": ["%PROGRAMFILES%\\Vecx"], "linux": ["/usr/bin", "~/.local/bin"] },
            "platforms": { "vectrex": { "args": ["{rom}"] } },
            "fullscreen_args": ["--fullscreen"]
        })];
        assert!(reg.extend("exemple", &emulators, &platforms).is_empty());
        assert!(reg.platform("vectrex").is_some());
        assert_eq!(reg.emulators_for("vectrex").len(), 1);
        // Un identifiant intégré ne peut pas être remplacé.
        let clash = vec![serde_json::json!({ "id": "snes", "name": "X", "short": "X", "extensions": [] })];
        assert!(!reg.extend("exemple", &[], &clash).is_empty());
    }

    #[test]
    fn custom_emulator_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path();
        let exe = data.join("Xemu Dev").join(if cfg!(windows) { "xemu.exe" } else { "xemu" });
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, b"x").unwrap();
        let pid = save_custom_platform(data, "Xbox", "iso, .XISO ; *.cso").unwrap();
        assert_eq!(pid, "perso-xbox");
        let (reg, warnings) = Registry::load(data);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(reg.platform("perso-xbox").unwrap().extensions, vec!["iso", "xiso", "cso"]);

        let e = CustomEmulator {
            name: "Xemu (dev)".into(),
            executable: exe.clone(),
            platforms: vec![pid.clone()],
            args: "-dvd_path {rom}".into(),
            fullscreen_args: "-full-screen".into(),
        };
        validate_custom(&e, &reg).unwrap();
        assert!(validate_custom(&CustomEmulator { args: "-dvd_path".into(), ..e.clone() }, &reg).is_err(), "{{rom}} obligatoire");
        assert!(validate_custom(&CustomEmulator { platforms: vec![], ..e.clone() }, &reg).is_err());
        let id = save_custom(data, None, &e).unwrap();
        assert_eq!(id, "perso-xemu-dev");
        assert_eq!(save_custom(data, None, &e).unwrap(), "perso-xemu-dev-2", "pas d'écrasement");
        assert!(save_custom(data, Some("retroarch"), &e).is_err(), "intégré non modifiable");

        let (reg, warnings) = Registry::load(data);
        assert!(warnings.is_empty(), "{warnings:?}");
        let def = reg.emulator(&id).unwrap();
        let d = detect(def, None).expect("détecté par son dossier");
        assert_eq!(d.program, exe);
        let rom = data.join("Halo 2.iso");
        std::fs::write(&rom, b"r").unwrap();
        let t = build_launch(def, &d, &pid, &rom, &Profile { extra_args: String::new(), fullscreen: true }).unwrap();
        match t {
            LaunchTarget::Process { program, args, .. } => {
                assert_eq!(program, exe);
                assert_eq!(args, vec!["-full-screen".to_string(), "-dvd_path".into(), rom.display().to_string()]);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(read_custom(data, &id).unwrap(), e);
        delete_custom(data, &id).unwrap();
        assert!(delete_custom(data, "retroarch").is_err());
        assert!(Registry::load(data).0.emulator(&id).is_none());
    }
}

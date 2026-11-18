//! Côté lanceur : nature de l'installation en cours et lancement de l'updater.

use crate::install::{self, Target};
use crate::recovery::Layout;
use crate::runner;
use crate::version::Version;
use std::path::{Path, PathBuf};

/// Ce que HUS peut faire pour l'installation en cours.
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    /// Installation utilisateur gérée par HUS : mise à jour automatique complète.
    Managed { layout: Layout, target: Target, updater: PathBuf },
    /// Paquet système (`.deb`) : mises à jour par le gestionnaire de paquets.
    PackageManager,
    /// Mise à jour automatique impossible (raison affichée) ; la nouvelle version est signalée.
    Unsupported(String),
}

impl Kind {
    pub fn describe(&self) -> String {
        match self {
            Kind::Managed { target: Target::Zip, layout, .. } => tr!("Installation utilisateur ({root})", root = layout.root.display()),
            Kind::Managed { target: Target::Appimage { .. }, layout, .. } => format!("AppImage ({})", layout.root.display()),
            Kind::Managed { target: Target::Migrate, layout, .. } => tr!("Installation utilisateur ({root}) : passera au format AppImage à la prochaine mise à jour", root = layout.root.display()),
            Kind::PackageManager => tr!("Paquet système : mises à jour par votre gestionnaire de paquets (apt…)"),
            Kind::Unsupported(why) => why.clone(),
        }
    }

    pub fn managed(&self) -> bool {
        matches!(self, Kind::Managed { .. })
    }
}

fn writable(dir: &Path) -> bool {
    let probe = dir.join(".hus").join(format!("probe-{}", std::process::id()));
    let ok = std::fs::create_dir_all(dir.join(".hus")).is_ok() && std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

fn dev_build(exe: &Path) -> bool {
    exe.components().any(|c| c.as_os_str() == "target") && exe.parent().is_some_and(|p| p.ends_with("debug") || p.ends_with("release"))
}

/// Détecte l'installation de ce processus.
pub fn detect() -> Kind {
    let Ok(exe) = std::env::current_exe() else { return Kind::Unsupported(tr!("emplacement de Halcyon inconnu")) };
    detect_from(&exe, std::env::var_os("APPIMAGE").map(PathBuf::from), std::env::var_os("APPDIR").map(PathBuf::from))
}

pub fn detect_from(exe: &Path, appimage: Option<PathBuf>, appdir: Option<PathBuf>) -> Kind {
    if cfg!(windows) {
        let Some(root) = exe.parent() else { return Kind::Unsupported(tr!("emplacement de Halcyon inconnu")) };
        if dev_build(exe) {
            return Kind::Unsupported(tr!("Version compilée localement : mise à jour manuelle"));
        }
        let updater = root.join(crate::security::WINDOWS_UPDATER);
        if !updater.is_file() {
            return Kind::Unsupported(tr!("halcyon-updater.exe absent : réinstallez Halcyon avec le dernier programme d'installation"));
        }
        if !writable(root) {
            return Kind::Unsupported(tr!("Installation pour tous les utilisateurs : mise à jour par le programme d'installation (droits d'administrateur)"));
        }
        return Kind::Managed { layout: Layout::new(root), target: Target::Zip, updater };
    }
    if let Some(img) = appimage.filter(|p| p.is_file()) {
        let (Some(root), Some(name)) = (img.parent(), img.file_name().and_then(|n| n.to_str())) else {
            return Kind::Unsupported(tr!("emplacement de l'AppImage inconnu"));
        };
        let Some(updater) = appdir.map(|d| d.join("usr/bin/halcyon-updater")).filter(|p| p.is_file()) else {
            return Kind::Unsupported(tr!("updater absent de cette AppImage"));
        };
        if !writable(root) {
            return Kind::Unsupported(tr!("{root} n'est pas modifiable : déplacez l'AppImage dans un dossier personnel", root = root.display()));
        }
        return Kind::Managed { layout: Layout::new(root), target: Target::Appimage { file: name.into() }, updater };
    }
    if exe.starts_with("/usr") || exe.starts_with("/opt") {
        return Kind::PackageManager;
    }
    if dev_build(exe) {
        return Kind::Unsupported(tr!("Version compilée localement : mise à jour manuelle"));
    }
    let Some(root) = exe.parent() else { return Kind::Unsupported(tr!("emplacement de Halcyon inconnu")) };
    if root.join(install::SETUP_RECORD).is_file() && root.join("halcyon-updater").is_file() && writable(root) {
        return Kind::Managed { layout: Layout::new(root), target: Target::Migrate, updater: root.join("halcyon-updater") };
    }
    Kind::Unsupported(tr!("Archive ou installation manuelle : téléchargez la nouvelle version depuis GitHub"))
}

/// Lance l'updater (copie dans `.hus/runner`) pour installer le paquet en attente, puis le
/// lanceur doit se fermer. `bootstrap` : updater extrait du nouveau paquet (trop ancien sinon).
pub fn launch_apply(layout: &Layout, updater: &Path, current: &Version) -> Result<(), String> {
    let pending = install::read_pending(layout).ok_or(tr!("aucune mise à jour en attente"))?;
    let manifest = std::fs::read(layout.pending().join(crate::manifest::MANIFEST_NAME)).ok();
    let min_updater = manifest
        .and_then(|b| crate::manifest::Manifest::parse(&b).ok())
        .and_then(|m| m.min_updater.parse::<Version>().ok())
        .unwrap_or_else(Version::current);
    // L'updater installé est celui de cette version de Halcyon (livrés ensemble).
    let source = if min_updater > *current {
        runner::updater_from_package(layout, &layout.pending().join(&pending.package), &pending.target)?
    } else {
        updater.to_path_buf()
    };
    let copy = runner::prepare_runner(layout, &source)?;
    let args = vec![
        "apply".to_string(),
        "--root".into(),
        layout.root.display().to_string(),
        "--current".into(),
        current.to_string(),
        "--wait-pid".into(),
        std::process::id().to_string(),
    ];
    runner::spawn_detached(&copy, &args, &layout.root).map(|_| ()).map_err(|e| tr!("démarrage de l'updater : {e}", e = e))?;
    crate::recovery::log(layout, &tr!("updater lancé pour {version} (lanceur {id} en fermeture)", version = pending.version, id = std::process::id()));
    Ok(())
}

/// Lance la reprise d'une transaction interrompue, puis le lanceur doit se fermer.
pub fn launch_recover(layout: &Layout, updater: &Path, target: &Target) -> Result<(), String> {
    let copy = runner::prepare_runner(layout, updater)?;
    let args = vec!["recover".to_string(), "--root".into(), layout.root.display().to_string(), "--target".into(), target.arg(), "--wait-pid".into(), std::process::id().to_string()];
    runner::spawn_detached(&copy, &args, &layout.root).map(|_| ()).map_err(|e| tr!("démarrage de l'updater : {e}", e = e))
}

/// Marqueur « installer au prochain démarrage » (choix explicite de l'utilisateur).
pub fn set_install_at_restart(layout: &Layout, on: bool) -> Result<(), String> {
    let m = layout.pending().join("at-restart");
    if on {
        std::fs::write(&m, b"1").map_err(|e| e.to_string())
    } else {
        let _ = std::fs::remove_file(&m);
        Ok(())
    }
}

pub fn install_at_restart(layout: &Layout) -> bool {
    layout.pending().join("at-restart").is_file()
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use super::*;

    #[cfg(unix)]
    #[test]
    fn detects_install_kinds() {
        let t = tempfile::tempdir().unwrap();
        // AppImage avec son updater.
        let img = t.path().join("opt/Halcyon.AppImage");
        let appdir = t.path().join("mnt");
        std::fs::create_dir_all(img.parent().unwrap()).unwrap();
        std::fs::create_dir_all(appdir.join("usr/bin")).unwrap();
        std::fs::write(&img, b"x").unwrap();
        std::fs::write(appdir.join("usr/bin/halcyon-updater"), b"x").unwrap();
        match detect_from(&appdir.join("usr/bin/halcyon"), Some(img.clone()), Some(appdir.clone())) {
            Kind::Managed { target, layout, .. } => {
                assert_eq!(target, Target::Appimage { file: "Halcyon.AppImage".into() });
                assert_eq!(layout.root, img.parent().unwrap());
            }
            k => panic!("{k:?}"),
        }
        assert_eq!(detect_from(Path::new("/usr/bin/halcyon"), None, None), Kind::PackageManager);
        assert!(matches!(detect_from(Path::new("/home/u/src/halcyon/target/release/halcyon"), None, None), Kind::Unsupported(_)));
        // Installation par halcyon-setup : migration vers l'AppImage.
        let setup = t.path().join("setup");
        std::fs::create_dir_all(&setup).unwrap();
        for f in ["halcyon", "halcyon-updater", "install.json"] {
            std::fs::write(setup.join(f), b"x").unwrap();
        }
        assert!(matches!(detect_from(&setup.join("halcyon"), None, None), Kind::Managed { target: Target::Migrate, .. }));
        std::fs::remove_file(setup.join("halcyon-updater")).unwrap();
        assert!(matches!(detect_from(&setup.join("halcyon"), None, None), Kind::Unsupported(_)));
    }
}

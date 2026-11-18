//! Updater réel (`halcyon-updater`) et faux Halcyon (`hus-fake-app`) en processus séparés :
//! installation, redémarrage, contrôle de santé, échec, délai dépassé, interruption pendant
//! l'installation, reprise, paquet altéré ou falsifié, retour vers une version ancienne,
//! conservation des paramètres et des fichiers inconnus, AppImage et migration (Linux).
#![cfg(feature = "fault-injection")]

use halcyon_update::core::Platform;
use halcyon_update::install::{self, Target};
use halcyon_update::recovery::{self, Layout};
use halcyon_update::runner::{self, Startup};
use halcyon_update::testkit;
use halcyon_update::{State, Version};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

const UPDATER: &str = env!("CARGO_BIN_EXE_halcyon-updater");
const FAKE: &str = env!("CARGO_BIN_EXE_hus-fake-app");

/// Faux Halcyon de version `v` (la version est lue à la fin du fichier).
fn fake_exe(v: &str) -> Vec<u8> {
    let mut b = std::fs::read(FAKE).unwrap();
    let mut tail = format!("HUSVER:{v}").into_bytes();
    tail.resize(64, 0);
    b.extend_from_slice(&tail);
    b
}

#[cfg(unix)]
fn fake_appimage(v: &str) -> Vec<u8> {
    let mut b = testkit::appimage_from_elf(&std::fs::read(FAKE).unwrap());
    let mut tail = format!("HUSVER:{v}").into_bytes();
    tail.resize(64, 0);
    b.extend_from_slice(&tail);
    b
}

fn zip_package(v: &str) -> Vec<u8> {
    let mut cursor = std::io::Cursor::new(Vec::new());
    {
        let mut w = zip::ZipWriter::new(&mut cursor);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated).unix_permissions(0o755);
        for (name, data) in [("Halcyon.exe", fake_exe(v)), ("halcyon-updater.exe", b"updater".to_vec()), (&format!("docs/notes-{v}.md") as &str, v.as_bytes().to_vec()), ("docs/GUIDE.md", format!("guide {v}").into_bytes())] {
            w.start_file(name, opts).unwrap();
            w.write_all(&data).unwrap();
        }
        w.finish().unwrap();
    }
    cursor.into_inner()
}

fn exe_version(path: &Path) -> String {
    let b = std::fs::read(path).unwrap();
    let t = String::from_utf8_lossy(&b[b.len() - 64..]).to_string();
    t.rsplit("HUSVER:").next().unwrap().trim_end_matches('\0').to_string()
}

struct Env {
    _t: tempfile::TempDir,
    root: PathBuf,
    appdata: PathBuf,
    log: PathBuf,
    kp: ed25519_compact::KeyPair,
}

impl Env {
    fn layout(&self) -> Layout {
        Layout::new(&self.root)
    }

    /// Installation Windows (ZIP) en 1.0.0, avec données portables, fichier inconnu et
    /// paramètres hors du dossier.
    fn zip_install() -> Env {
        let t = tempfile::tempdir().unwrap();
        let root = t.path().join("Halcyon");
        std::fs::create_dir_all(root.join("docs")).unwrap();
        std::fs::create_dir_all(root.join("data")).unwrap();
        write_exec(&root.join("Halcyon.exe"), &fake_exe("1.0.0"));
        std::fs::write(root.join("halcyon-updater.exe"), b"old updater").unwrap();
        std::fs::write(root.join("docs/notes-1.0.0.md"), b"1.0.0").unwrap();
        std::fs::write(root.join("docs/GUIDE.md"), b"guide 1.0.0").unwrap();
        std::fs::write(root.join("data/halcyon.db"), b"bibliotheque").unwrap();
        std::fs::write(root.join("portable.txt"), b"").unwrap();
        std::fs::write(root.join("keep-me.txt"), b"fichier de l'utilisateur").unwrap();
        let installed = recovery::Installed { version: "1.0.0".into(), files: vec!["Halcyon.exe".into(), "halcyon-updater.exe".into(), "docs/notes-1.0.0.md".into(), "docs/GUIDE.md".into()] };
        std::fs::write(root.join(recovery::INSTALLED), serde_json::to_vec(&installed).unwrap()).unwrap();
        let appdata = t.path().join("appdata");
        std::fs::create_dir_all(&appdata).unwrap();
        std::fs::write(appdata.join("settings.json"), br#"{"update_channel":"beta"}"#).unwrap();
        let log = t.path().join("fake.log");
        Env { root, appdata, log, kp: testkit::test_key(7), _t: t }
    }

    fn stage(&self, version: &str, platform: Platform, payload: &[u8], target: Target, key: &ed25519_compact::KeyPair) {
        let m = testkit::manifest_for(version, platform, payload);
        let bytes = serde_json::to_vec_pretty(&m).unwrap();
        let file = self.root.join(".hus").join(&m.packages[0].name);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, payload).unwrap();
        install::stage(&self.layout(), &bytes, &testkit::sign(key, &bytes), &file, &version.parse().unwrap(), &target).unwrap();
    }

    fn updater(&self, args: &[&str], env: &[(&str, &str)]) -> i32 {
        let mut cmd = Command::new(UPDATER);
        cmd.args(args).env("HUS_TEST_PUBKEY", testkit::pubkey_b64(&self.kp)).env("HUS_FAKE_LOG", &self.log);
        for (k, v) in env {
            cmd.env(k, v);
        }
        cmd.status().unwrap().code().unwrap_or(-1)
    }

    fn apply(&self, current: &str, env: &[(&str, &str)]) -> i32 {
        self.updater(&["apply", "--root", self.root.to_str().unwrap(), "--current", current, "--health-timeout", "6"], env)
    }

    fn state(&self) -> recovery::Record {
        recovery::load(&self.layout())
    }

    fn fake_log_contains(&self, needle: &str) -> bool {
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            if std::fs::read_to_string(&self.log).unwrap_or_default().contains(needle) {
                return true;
            }
            if Instant::now() > until {
                return false;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    fn assert_user_data_intact(&self) {
        assert_eq!(std::fs::read(self.root.join("data/halcyon.db")).unwrap(), b"bibliotheque");
        assert!(self.root.join("portable.txt").is_file());
        assert_eq!(std::fs::read(self.root.join("keep-me.txt")).unwrap(), b"fichier de l'utilisateur");
        assert_eq!(std::fs::read(self.appdata.join("settings.json")).unwrap(), br#"{"update_channel":"beta"}"#);
    }

    fn assert_is_v100(&self) {
        assert_eq!(exe_version(&self.root.join("Halcyon.exe")), "1.0.0");
        assert_eq!(std::fs::read(self.root.join("docs/notes-1.0.0.md")).unwrap(), b"1.0.0");
        assert_eq!(std::fs::read(self.root.join("docs/GUIDE.md")).unwrap(), b"guide 1.0.0");
        assert_eq!(std::fs::read(self.root.join("halcyon-updater.exe")).unwrap(), b"old updater");
        assert!(!self.root.join("docs/notes-1.1.0.md").exists());
        assert!(!install::interrupted(&self.layout()), "journal de transaction soldé");
        self.assert_user_data_intact();
    }
}

fn write_exec(p: &Path, bytes: &[u8]) {
    std::fs::write(p, bytes).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}

#[test]
fn update_restarts_new_version_and_keeps_settings() {
    let e = Env::zip_install();
    e.stage("1.1.0", Platform::WindowsX64, &zip_package("1.1.0"), Target::Zip, &e.kp);
    assert_eq!(e.state().state, State::Staging);
    assert_eq!(e.apply("1.0.0", &[]), 0);
    assert_eq!(exe_version(&e.root.join("Halcyon.exe")), "1.1.0");
    assert!(e.root.join("docs/notes-1.1.0.md").is_file());
    assert!(!e.root.join("docs/notes-1.0.0.md").exists(), "fichier abandonné par la nouvelle version retiré");
    assert_eq!(std::fs::read(e.root.join("docs/GUIDE.md")).unwrap(), b"guide 1.1.0");
    e.assert_user_data_intact();
    let s = e.state();
    assert_eq!(s.state, State::Completed);
    assert_eq!(s.highest.as_deref(), Some("1.1.0"));
    assert!(!s.acknowledged);
    // Redémarrage : la nouvelle version a été lancée pour le contrôle de santé.
    assert!(e.fake_log_contains("start 1.1.0 --hus-health"));
    // Version précédente conservée ; plus rien en attente.
    assert_eq!(exe_version(&e.layout().previous().join("1.0.0/Halcyon.exe")), "1.0.0");
    assert!(install::read_pending(&e.layout()).is_none());
    assert_eq!(recovery::read_installed(&e.layout()).unwrap().version, "1.1.0");
    let log = std::fs::read_to_string(e.layout().log_file()).unwrap();
    assert!(log.contains("terminée"), "{log}");
}

#[test]
fn failed_start_rolls_back_and_relaunches_previous() {
    let e = Env::zip_install();
    e.stage("1.1.0", Platform::WindowsX64, &zip_package("1.1.0"), Target::Zip, &e.kp);
    assert_eq!(e.apply("1.0.0", &[("HUS_FAKE_CRASH_VERSION", "1.1.0")]), 2);
    e.assert_is_v100();
    let s = e.state();
    assert_eq!(s.state, State::RolledBack);
    assert_eq!(s.blocked.as_deref(), Some("1.1.0"), "pas de nouvel essai automatique");
    assert!(s.message.contains("restaurée"));
    assert!(e.fake_log_contains("start 1.0.0 --hus-rolled-back"), "ancienne version relancée");
}

#[test]
fn silent_new_version_times_out_and_rolls_back() {
    let e = Env::zip_install();
    e.stage("1.1.0", Platform::WindowsX64, &zip_package("1.1.0"), Target::Zip, &e.kp);
    let t0 = Instant::now();
    assert_eq!(e.apply("1.0.0", &[("HUS_FAKE_SILENT_VERSION", "1.1.0")]), 2);
    assert!(t0.elapsed() >= Duration::from_secs(5));
    e.assert_is_v100();
}

#[test]
fn interruption_during_install_is_recovered() {
    let e = Env::zip_install();
    e.stage("1.1.0", Platform::WindowsX64, &zip_package("1.1.0"), Target::Zip, &e.kp);
    // Arrêt brutal (coupure, updater tué) après le premier fichier remplacé.
    assert_eq!(e.apply("1.0.0", &[("HUS_FAKE_CRASH_VERSION", "none"), ("HUS_FAULT", "mid-install")]), 86);
    assert!(install::interrupted(&e.layout()));
    assert_eq!(e.state().state, State::Installing);
    // Au démarrage suivant, le lanceur demande la reprise…
    assert_eq!(runner::on_launcher_start(&e.layout(), None), Startup::NeedsRecovery);
    // … que l'updater exécute (lui-même interrompu une fois pendant le retour arrière).
    let root = e.root.to_str().unwrap().to_string();
    assert_eq!(e.updater(&["recover", "--root", &root, "--target", "zip", "--no-relaunch"], &[("HUS_FAULT", "mid-rollback")]), 86);
    assert_eq!(e.updater(&["recover", "--root", &root, "--target", "zip", "--no-relaunch"], &[]), 2);
    e.assert_is_v100();
    assert_eq!(e.state().state, State::RolledBack);
    assert_eq!(runner::on_launcher_start(&e.layout(), None), Startup::Normal);
}

#[test]
fn interruption_after_install_is_confirmed_by_next_start() {
    let e = Env::zip_install();
    e.stage("1.1.0", Platform::WindowsX64, &zip_package("1.1.0"), Target::Zip, &e.kp);
    assert_eq!(e.apply("1.0.0", &[("HUS_FAULT", "before-health")]), 86);
    assert_eq!(e.state().state, State::HealthCheck);
    // La nouvelle version démarre normalement : la transaction est conclue.
    assert_eq!(runner::on_launcher_start(&e.layout(), None), Startup::Finalized);
    assert_eq!(e.state().state, State::Completed);
    assert_eq!(exe_version(&e.root.join("Halcyon.exe")), "1.1.0");
    e.assert_user_data_intact();
}

#[test]
fn tampered_or_forged_or_older_packages_change_nothing() {
    // Paquet modifié après sa mise en attente.
    let e = Env::zip_install();
    e.stage("1.1.0", Platform::WindowsX64, &zip_package("1.1.0"), Target::Zip, &e.kp);
    let pkg = e.layout().pending().join("Halcyon-1.1.0-Windows-x64.zip");
    let mut b = std::fs::read(&pkg).unwrap();
    let n = b.len() / 2;
    b[n] ^= 0x55;
    std::fs::write(&pkg, b).unwrap();
    assert_eq!(e.apply("1.0.0", &[]), 1);
    e.assert_is_v100();
    assert_eq!(e.state().state, State::Failed);
    assert!(e.state().message.contains("altéré"), "{}", e.state().message);
    assert!(install::read_pending(&e.layout()).is_none(), "paquet refusé retiré");

    // Manifeste signé par une autre clé.
    let e = Env::zip_install();
    e.stage("1.1.0", Platform::WindowsX64, &zip_package("1.1.0"), Target::Zip, &testkit::test_key(42));
    assert_eq!(e.apply("1.0.0", &[]), 1);
    e.assert_is_v100();
    assert!(e.state().message.contains("signature"));

    // Retour vers une version plus ancienne (paquet correctement signé).
    let e = Env::zip_install();
    e.stage("0.9.0", Platform::WindowsX64, &zip_package("0.9.0"), Target::Zip, &e.kp);
    assert_eq!(e.apply("1.0.0", &[]), 3);
    e.assert_is_v100();
    assert!(e.state().message.contains("pas plus récente"), "{}", e.state().message);
}

#[test]
fn single_updater_at_a_time() {
    let e = Env::zip_install();
    e.stage("1.1.0", Platform::WindowsX64, &zip_package("1.1.0"), Target::Zip, &e.kp);
    let _held = recovery::Lock::acquire(&e.layout()).unwrap();
    // Le verrou appartient à ce processus de test, vivant : le second updater renonce.
    assert_eq!(e.apply("1.0.0", &[]), 1);
    e.assert_is_v100();
}

#[cfg(unix)]
#[test]
fn waits_for_launcher_exit() {
    let e = Env::zip_install();
    e.stage("1.1.0", Platform::WindowsX64, &zip_package("1.1.0"), Target::Zip, &e.kp);
    let mut launcher = Command::new("sleep").arg("2").spawn().unwrap();
    let pid = launcher.id().to_string();
    let t0 = Instant::now();
    let root = e.root.to_str().unwrap().to_string();
    let code = e.updater(&["apply", "--root", &root, "--current", "1.0.0", "--wait-pid", &pid, "--health-timeout", "6"], &[]);
    let _ = launcher.wait();
    assert_eq!(code, 0);
    assert!(t0.elapsed() >= Duration::from_millis(1800), "l'updater a attendu la fermeture du lanceur");
}

#[cfg(unix)]
mod linux {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn appimage_install(name: &str) -> Env {
        let e = Env::zip_install();
        for f in ["Halcyon.exe", "halcyon-updater.exe", recovery::INSTALLED] {
            std::fs::remove_file(e.root.join(f)).unwrap();
        }
        std::fs::remove_dir_all(e.root.join("docs")).unwrap();
        write_exec(&e.root.join(name), &fake_appimage("1.0.0"));
        e
    }

    #[test]
    fn appimage_is_replaced_atomically_and_previous_kept() {
        let e = appimage_install("Halcyon.AppImage");
        e.stage("1.1.0", Platform::LinuxX64, &fake_appimage("1.1.0"), Target::Appimage { file: "Halcyon.AppImage".into() }, &e.kp);
        assert_eq!(e.apply("1.0.0", &[]), 0);
        let app = e.root.join("Halcyon.AppImage");
        assert_eq!(exe_version(&app), "1.1.0");
        assert_eq!(std::fs::metadata(&app).unwrap().permissions().mode() & 0o777, 0o755);
        assert_eq!(exe_version(&e.layout().previous().join("1.0.0/Halcyon.AppImage")), "1.0.0");
        e.assert_user_data_intact();
    }

    #[test]
    fn appimage_failed_start_is_rolled_back() {
        let e = appimage_install("Halcyon.AppImage");
        e.stage("1.1.0", Platform::LinuxX64, &fake_appimage("1.1.0"), Target::Appimage { file: "Halcyon.AppImage".into() }, &e.kp);
        assert_eq!(e.apply("1.0.0", &[("HUS_FAKE_CRASH_VERSION", "1.1.0")]), 2);
        assert_eq!(exe_version(&e.root.join("Halcyon.AppImage")), "1.0.0");
        assert!(e.fake_log_contains("start 1.0.0 --hus-rolled-back"));
    }

    #[test]
    fn non_appimage_package_is_refused() {
        let e = appimage_install("Halcyon.AppImage");
        e.stage("1.1.0", Platform::LinuxX64, b"#!/bin/sh\necho pirate\n", Target::Appimage { file: "Halcyon.AppImage".into() }, &e.kp);
        assert_eq!(e.apply("1.0.0", &[]), 1);
        assert_eq!(exe_version(&e.root.join("Halcyon.AppImage")), "1.0.0");
        assert!(e.state().message.contains("AppImage"), "{}", e.state().message);
    }

    /// Installation par `halcyon-setup` (binaire `halcyon`, fiche `install.json`) : passage à
    /// l'AppImage, `halcyon` devient un lien (raccourcis conservés), fiche mise à jour.
    #[test]
    fn setup_install_migrates_to_appimage_and_can_roll_back() {
        for crash in [false, true] {
            let e = appimage_install("halcyon");
            std::fs::write(e.root.join("halcyon.png"), b"icone").unwrap();
            let record = serde_json::json!({"version": "1.0.0", "platform": "linux-x86_64", "installed_at": 0, "options": {}, "files": [{"path": "halcyon", "size": 1, "sha256": "x"}, {"path": "halcyon.png", "size": 5, "sha256": "y"}], "integration": ["/home/u/.local/bin/halcyon"]});
            std::fs::write(e.root.join("install.json"), serde_json::to_vec(&record).unwrap()).unwrap();
            e.stage("1.1.0", Platform::LinuxX64, &fake_appimage("1.1.0"), Target::Migrate, &e.kp);
            let env: &[(&str, &str)] = if crash { &[("HUS_FAKE_CRASH_VERSION", "1.1.0")] } else { &[] };
            let code = e.apply("1.0.0", env);
            let main = e.root.join("halcyon");
            if crash {
                assert_eq!(code, 2);
                assert!(!main.symlink_metadata().unwrap().file_type().is_symlink(), "binaire d'origine restauré");
                assert_eq!(exe_version(&main), "1.0.0");
                assert!(!e.root.join("Halcyon.AppImage").exists());
                let rec: serde_json::Value = serde_json::from_slice(&std::fs::read(e.root.join("install.json")).unwrap()).unwrap();
                assert_eq!(rec["version"], "1.0.0");
            } else {
                assert_eq!(code, 0);
                assert_eq!(std::fs::read_link(&main).unwrap(), PathBuf::from("Halcyon.AppImage"));
                assert_eq!(exe_version(&main), "1.1.0");
                assert!(e.root.join("halcyon.png").is_file(), "icône conservée");
                let rec: serde_json::Value = serde_json::from_slice(&std::fs::read(e.root.join("install.json")).unwrap()).unwrap();
                assert_eq!(rec["version"], "1.1.0");
                assert_eq!(rec["integration"][0], "/home/u/.local/bin/halcyon", "intégration de l'installateur conservée");
                let paths: Vec<&str> = rec["files"].as_array().unwrap().iter().map(|f| f["path"].as_str().unwrap()).collect();
                assert!(paths.contains(&"Halcyon.AppImage") && paths.contains(&"halcyon") && paths.contains(&"halcyon.png"));
            }
            e.assert_user_data_intact();
        }
    }
}

#[test]
fn version_floor_from_state_blocks_reinstalling_older() {
    let e = Env::zip_install();
    let mut r = recovery::load(&e.layout());
    r.highest = Some("1.2.0".into());
    recovery::save(&e.layout(), &r).unwrap();
    // La fiche annonce 1.0.0, mais 1.2.0 a déjà été installée ici : 1.1.0 serait un retour arrière.
    e.stage("1.1.0", Platform::WindowsX64, &zip_package("1.1.0"), Target::Zip, &e.kp);
    assert_eq!(e.apply("1.0.0", &[]), 3);
    e.assert_is_v100();
    assert!(e.state().message.contains("minimale de sécurité"), "{}", e.state().message);
    assert!(Version::current() >= "1.0.0-alpha.1".parse().unwrap());
}

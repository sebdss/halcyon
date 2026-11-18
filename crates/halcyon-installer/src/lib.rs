//! Installateur de Halcyon : moteur sans interface (testé) et charge utile autonome.
//! L'interface (Slint) est dans `main.rs`.

#[macro_use]
extern crate halcyon_i18n;

pub mod engine;
pub mod payload;
pub mod platform;

#[cfg(test)]
mod tests {
    use crate::engine::*;
    use crate::payload::{pack, Manifest, Payload};
    use crate::platform::{self, Env};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::AtomicBool;

    fn manifest(version: &str) -> Manifest {
        Manifest {
            product: "Halcyon".into(),
            version: version.into(),
            platform: platform::current_platform(),
            main: platform::main_exe_name().into(),
            min_glibc: None,
            files: vec![],
        }
    }

    /// Fabrique un installateur de test : `files` (chemin, contenu) + programme principal.
    fn build(root: &Path, version: &str, files: &[(&str, &str)]) -> PathBuf {
        let src = root.join(format!("src-{version}"));
        let _ = std::fs::remove_dir_all(&src);
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join(platform::main_exe_name()), format!("halcyon {version}")).unwrap();
        for (p, c) in files {
            let f = src.join(p);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, c).unwrap();
        }
        let stub = root.join("stub");
        std::fs::write(&stub, b"STUB-EXECUTABLE").unwrap();
        let out = root.join(format!("setup-{version}"));
        pack(&stub, &src, manifest(version), &out).unwrap();
        out
    }

    fn opts(env: &Env) -> Options {
        Options { dir: env.default_dir.clone(), menu_entry: true, desktop_shortcut: true, autostart: false, all_users: false }
    }

    fn install(setup: &Path, env: &Env, mode: Mode, prev: Option<&Record>) -> Result<Report, String> {
        let p = Payload::from_exe(setup)?.unwrap();
        let mut events = Vec::new();
        let r = run(&p, &opts(env), env, mode, prev, Some(setup), &AtomicBool::new(false), &mut |e| events.push(e));
        // La progression va de 0 au total, sans jamais le dépasser.
        for e in &events {
            if let Event::Bytes(d, t) = e {
                assert!(d <= t);
            }
        }
        r
    }

    fn installed(env: &Env) -> (Record, Health) {
        match detect(env) {
            Found::Installed { record, health } => (record, health),
            other => panic!("installation non détectée : {other:?}"),
        }
    }

    #[test]
    fn payload_round_trip_and_stub() {
        let t = tempfile::tempdir().unwrap();
        let setup = build(t.path(), "0.4.1", &[("docs/GUIDE.md", "guide")]);
        let p = Payload::from_exe(&setup).unwrap().unwrap();
        assert_eq!(p.manifest.version, "0.4.1");
        assert_eq!(p.manifest.files.len(), 2);
        assert!(Payload::from_exe(&t.path().join("stub")).unwrap().is_none(), "programme sans charge utile");
        let bytes = std::fs::read(&setup).unwrap();
        assert_eq!(&bytes[..crate::payload::strip_len(&bytes)], b"STUB-EXECUTABLE");
        // Fichier tronqué : refusé clairement.
        let cut = t.path().join("cut");
        std::fs::write(&cut, &bytes[..bytes.len() - 3]).unwrap();
        assert!(Payload::from_exe(&cut).unwrap().is_none() || Payload::from_exe(&cut).is_err());
    }

    #[test]
    fn rejects_unsafe_paths() {
        use crate::payload::safe_relative;
        assert!(safe_relative("../etc/passwd").is_none());
        assert!(safe_relative("/etc/passwd").is_none());
        assert!(safe_relative("a/../../b").is_none());
        assert!(safe_relative("C:/x").is_none());
        assert_eq!(safe_relative("docs/GUIDE.md"), Some(PathBuf::from("docs").join("GUIDE.md")));
    }

    #[test]
    fn fresh_install_then_detect() {
        let t = tempfile::tempdir().unwrap();
        let env = Env::rooted(&t.path().join("home"));
        assert_eq!(detect(&env), Found::Nothing);
        let setup = build(t.path(), "0.4.1", &[("halcyon.png", "png"), ("docs/GUIDE.md", "guide")]);
        let r = install(&setup, &env, Mode::Install, None).unwrap();
        assert_eq!(r.written, 3);
        let (rec, health) = installed(&env);
        assert!(health.ok());
        assert_eq!(rec.version, "0.4.1");
        let dir = &env.default_dir;
        assert_eq!(std::fs::read_to_string(dir.join(platform::main_exe_name())).unwrap(), "halcyon 0.4.1");
        assert_eq!(std::fs::read(dir.join(platform::maintenance_exe_name())).unwrap(), b"STUB-EXECUTABLE");
        assert!(!dir.join(".halcyon-staging").exists() && !dir.join(".halcyon-backup").exists());
        #[cfg(not(windows))]
        {
            let link = env.bin_dir.join("halcyon");
            assert_eq!(std::fs::read_link(&link).unwrap(), dir.join("halcyon"));
            let entry = std::fs::read_to_string(env.data_home.join("applications/halcyon.desktop")).unwrap();
            assert!(entry.contains(&dir.display().to_string()));
            assert!(env.desktop.as_ref().unwrap().join("halcyon.desktop").is_file());
            assert!(!env.config_home.join("autostart/halcyon.desktop").exists(), "démarrage automatique non demandé");
            assert_eq!(recommend(&detect(&env), "0.4.1"), Mode::Reinstall);
            assert_eq!(recommend(&detect(&env), "0.5.0"), Mode::Update);
        }
    }

    #[test]
    fn update_keeps_user_files_and_drops_obsolete_ones() {
        let t = tempfile::tempdir().unwrap();
        let env = Env::rooted(&t.path().join("home"));
        let v1 = build(t.path(), "0.4.0", &[("old.txt", "ancien"), ("docs/GUIDE.md", "v1")]);
        install(&v1, &env, Mode::Install, None).unwrap();
        let dir = env.default_dir.clone();
        std::fs::create_dir_all(dir.join("data")).unwrap();
        std::fs::write(dir.join("data/library.db"), "mes jeux").unwrap();
        std::fs::write(dir.join("portable.txt"), "").unwrap();
        let (prev, _) = installed(&env);
        let v2 = build(t.path(), "0.4.1", &[("docs/GUIDE.md", "v2")]);
        assert_eq!(recommend(&detect(&env), "0.4.1"), Mode::Update);
        let r = install(&v2, &env, Mode::Update, Some(&prev)).unwrap();
        assert!(r.replaced >= 2);
        assert_eq!(std::fs::read_to_string(dir.join("docs/GUIDE.md")).unwrap(), "v2");
        assert!(!dir.join("old.txt").exists(), "fichier de l'ancienne version retiré");
        assert_eq!(std::fs::read_to_string(dir.join("data/library.db")).unwrap(), "mes jeux", "données conservées");
        assert!(dir.join("portable.txt").exists());
        assert_eq!(installed(&env).0.version, "0.4.1");
    }

    #[test]
    fn repair_rewrites_only_damaged_files() {
        let t = tempfile::tempdir().unwrap();
        let env = Env::rooted(&t.path().join("home"));
        let setup = build(t.path(), "0.4.1", &[("a.txt", "AAAA"), ("b.txt", "BBBB"), ("c.txt", "CCCC")]);
        install(&setup, &env, Mode::Install, None).unwrap();
        let dir = env.default_dir.clone();
        std::fs::write(dir.join("a.txt"), "XXXX").unwrap(); // même taille, contenu altéré
        std::fs::remove_file(dir.join("b.txt")).unwrap();
        let (rec, health) = installed(&env);
        assert_eq!(health.corrupt, vec!["a.txt".to_string()]);
        assert_eq!(health.missing, vec!["b.txt".to_string()]);
        assert_eq!(recommend(&detect(&env), "0.4.1"), Mode::Repair);
        let r = install(&setup, &env, Mode::Repair, Some(&rec)).unwrap();
        assert_eq!(r.written, 2);
        assert!(installed(&env).1.ok());
        assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "AAAA");
    }

    #[test]
    fn interrupted_transaction_is_rolled_back() {
        let t = tempfile::tempdir().unwrap();
        let env = Env::rooted(&t.path().join("home"));
        let setup = build(t.path(), "0.4.0", &[("a.txt", "ancien")]);
        install(&setup, &env, Mode::Install, None).unwrap();
        let dir = env.default_dir.clone();
        // Coupure simulée en pleine mise en place : a.txt déplacé, nouveau fichier posé.
        std::fs::create_dir_all(dir.join(".halcyon-backup")).unwrap();
        std::fs::rename(dir.join("a.txt"), dir.join(".halcyon-backup/a.txt")).unwrap();
        std::fs::write(dir.join("a.txt"), "à moitié").unwrap();
        std::fs::write(dir.join("new.txt"), "nouveau").unwrap();
        std::fs::write(dir.join(".halcyon-transaction.json"), r#"{"moved":["a.txt"],"created":["new.txt"]}"#).unwrap();
        assert_eq!(detect(&env), Found::Interrupted { dir: dir.clone() });
        recover(&dir).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "ancien");
        assert!(!dir.join("new.txt").exists());
        assert!(installed(&env).1.ok());
    }

    #[test]
    fn failed_placement_restores_previous_version() {
        let t = tempfile::tempdir().unwrap();
        let env = Env::rooted(&t.path().join("home"));
        let v1 = build(t.path(), "0.4.0", &[("a.txt", "v1")]);
        install(&v1, &env, Mode::Install, None).unwrap();
        let (prev, _) = installed(&env);
        let dir = env.default_dir.clone();
        // Obstacle : un dossier non vide là où la nouvelle version veut poser un fichier.
        std::fs::create_dir_all(dir.join("z.txt/bloque")).unwrap();
        let v2 = build(t.path(), "0.4.1", &[("a.txt", "v2"), ("z.txt", "fichier")]);
        // Échec au milieu de la mise en place : a.txt déjà remplacé doit revenir en v1.
        let err = install(&v2, &env, Mode::Update, Some(&prev)).unwrap_err();
        assert!(err.contains("rétablie"), "{err}");
        assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "v1");
        assert!(dir.join("z.txt/bloque").is_dir(), "dossier de l'utilisateur intact");
        assert!(installed(&env).1.ok());
        assert!(!dir.join(".halcyon-transaction.json").exists());
    }

    #[test]
    fn cancel_before_placement_changes_nothing() {
        let t = tempfile::tempdir().unwrap();
        let env = Env::rooted(&t.path().join("home"));
        let setup = build(t.path(), "0.4.1", &[("a.txt", "a")]);
        let p = Payload::from_exe(&setup).unwrap().unwrap();
        let cancel = AtomicBool::new(true);
        let r = run(&p, &opts(&env), &env, Mode::Install, None, None, &cancel, &mut |_| {});
        assert!(r.unwrap_err().contains("annulée"));
        assert!(!env.default_dir.join(platform::main_exe_name()).exists());
        assert_eq!(detect(&env), Found::Nothing);
    }

    #[test]
    fn uninstall_keeps_data_unless_asked() {
        let t = tempfile::tempdir().unwrap();
        let env = Env::rooted(&t.path().join("home"));
        let setup = build(t.path(), "0.4.1", &[("docs/GUIDE.md", "g")]);
        install(&setup, &env, Mode::Install, None).unwrap();
        for d in env.user_data.iter().chain(&env.user_cache) {
            std::fs::create_dir_all(d).unwrap();
            std::fs::write(d.join("x"), "x").unwrap();
        }
        std::fs::write(env.default_dir.join("notes-perso.txt"), "à moi").unwrap();
        // État des mises à jour automatiques (HUS) : retiré avec l'installation.
        std::fs::create_dir_all(env.default_dir.join(".hus/previous/0.4.0")).unwrap();
        std::fs::write(env.default_dir.join(".hus/state.json"), "{}").unwrap();
        std::fs::write(env.default_dir.join(".hus-installed.json"), "{}").unwrap();
        let (rec, _) = installed(&env);
        let r = uninstall(&rec, &env, &UninstallOptions { remove_cache: true, remove_data: false }, &mut |_| {}).unwrap();
        assert!(r.written >= 2);
        assert!(!env.default_dir.join(platform::main_exe_name()).exists());
        assert!(env.default_dir.join("notes-perso.txt").exists(), "fichier inconnu conservé");
        assert!(!env.default_dir.join(".hus").exists() && !env.default_dir.join(".hus-installed.json").exists(), "état HUS retiré");
        assert!(!r.warnings.is_empty());
        assert!(env.user_data[0].join("x").exists(), "données conservées");
        assert!(!env.user_cache[0].exists(), "cache supprimé sur demande");
        #[cfg(not(windows))]
        {
            assert!(env.bin_dir.join("halcyon").symlink_metadata().is_err());
            assert!(!env.data_home.join("applications/halcyon.desktop").exists());
        }
        assert_eq!(detect(&env), Found::Nothing);
        // Réinstallation puis désinstallation complète.
        install(&setup, &env, Mode::Install, None).unwrap();
        let (rec, _) = installed(&env);
        uninstall(&rec, &env, &UninstallOptions { remove_cache: false, remove_data: true }, &mut |_| {}).unwrap();
        assert!(!env.user_data[0].exists());
    }

    #[cfg(not(windows))]
    #[test]
    fn legacy_archive_install_is_taken_over() {
        let t = tempfile::tempdir().unwrap();
        let env = Env::rooted(&t.path().join("home"));
        std::fs::create_dir_all(&env.bin_dir).unwrap();
        std::fs::write(env.bin_dir.join("halcyon"), "ancienne copie").unwrap();
        assert!(matches!(detect(&env), Found::Unmanaged { origin: Origin::ArchiveScript, .. }));
        let setup = build(t.path(), "0.4.1", &[]);
        install(&setup, &env, Mode::Update, None).unwrap();
        assert!(env.bin_dir.join("halcyon").symlink_metadata().unwrap().file_type().is_symlink());
        assert!(matches!(detect(&env), Found::Installed { .. }));
    }

    #[test]
    fn checks_report_blocking_problems() {
        assert_eq!(space_check(Some(10), 100).level, Level::Error);
        assert_eq!(space_check(Some(1000), 100).level, Level::Ok);
        assert_eq!(space_check(None, 100).level, Level::Warning);
        assert_eq!(required_space(1000), 1100);
        #[cfg(target_os = "linux")]
        assert!(writable(Path::new("/proc/halcyon-test")).is_err(), "écriture refusée");
        assert!(writable(&std::env::temp_dir()).is_ok());
        assert_eq!(compare_versions("0.4.0", "0.4.1"), Some(std::cmp::Ordering::Less));
        assert_eq!(compare_versions("v0.10.0", "0.9.9"), Some(std::cmp::Ordering::Greater));
        assert_eq!(compare_versions("x", "0.1"), None);
        assert_eq!(compare_versions("0.5.0", "1.0.0-alpha.1"), Some(std::cmp::Ordering::Less), "mise à jour vers une pré-version");
        assert_eq!(compare_versions("1.0.0-alpha.1", "1.0.0"), Some(std::cmp::Ordering::Less));
        assert_eq!(compare_versions("1.0.0", "1.0.0"), Some(std::cmp::Ordering::Equal));
    }

    #[test]
    fn system_checks_on_this_machine() {
        let t = tempfile::tempdir().unwrap();
        let env = Env::rooted(t.path());
        let setup = build(t.path(), "0.4.1", &[]);
        let p = Payload::from_exe(&setup).unwrap().unwrap();
        let checks = system_checks(&p, &opts(&env), &Found::Nothing, Mode::Install);
        assert!(checks.iter().any(|c| c.label == "Espace disque"));
        assert!(checks.iter().filter(|c| c.label == "Système").all(|c| c.level == Level::Ok));
        // Dossier non inscriptible : seul Linux en fournit un sûr (/proc).
        #[cfg(target_os = "linux")]
        {
            let mut o = opts(&env);
            o.dir = PathBuf::from("/proc/halcyon-test");
            let checks = system_checks(&p, &o, &Found::Nothing, Mode::Install);
            assert!(checks.iter().any(|c| c.label == "Permissions" && c.level == Level::Error));
        }
    }

    #[test]
    fn maintenance_repair_recreates_integration_only() {
        let t = tempfile::tempdir().unwrap();
        let env = Env::rooted(&t.path().join("home"));
        let setup = build(t.path(), "0.4.1", &[("GUIDE.md", "g")]);
        install(&setup, &env, Mode::Install, None).unwrap();
        let (rec, _) = installed(&env);
        #[cfg(not(windows))]
        {
            std::fs::remove_file(env.data_home.join("applications/halcyon.desktop")).unwrap();
            repair_integration(&rec, &env, &mut |_| {}).unwrap();
            assert!(env.data_home.join("applications/halcyon.desktop").exists(), "entrée de menu recréée");
        }
        // Fichier abîmé : la réparation sans charge utile refuse et explique.
        std::fs::write(env.default_dir.join("GUIDE.md"), "modifié").unwrap();
        let e = repair_integration(&rec, &env, &mut |_| {}).unwrap_err();
        assert!(e.contains("GUIDE.md") && e.contains("0.4.1"), "{e}");
        assert_eq!(std::fs::read_to_string(env.default_dir.join("GUIDE.md")).unwrap(), "modifié", "rien n'est écrasé");
    }

    #[cfg(not(windows))]
    #[test]
    fn uninstall_removes_only_our_game_shortcuts() {
        let t = tempfile::tempdir().unwrap();
        let env = Env::rooted(&t.path().join("home"));
        let setup = build(t.path(), "0.4.1", &[]);
        install(&setup, &env, Mode::Install, None).unwrap();
        let apps = env.data_home.join("applications");
        let exe = env.default_dir.join("halcyon");
        std::fs::write(apps.join("halcyon-jeu-1.desktop"), format!("[Desktop Entry]\nExec=\"{}\" --launch 1\n", exe.display())).unwrap();
        std::fs::write(apps.join("halcyon-jeu-2.desktop"), "[Desktop Entry]\nExec=/usr/bin/halcyon --launch 2\n").unwrap();
        std::fs::write(apps.join("autre.desktop"), "x").unwrap();
        let (rec, _) = installed(&env);
        uninstall(&rec, &env, &UninstallOptions::default(), &mut |_| {}).unwrap();
        assert!(!apps.join("halcyon-jeu-1.desktop").exists());
        assert!(apps.join("halcyon-jeu-2.desktop").exists(), "raccourci d'une autre installation conservé");
        assert!(apps.join("autre.desktop").exists());
    }

    /// Exécutable PE minimal avec une « signature » Authenticode ajoutée après la charge
    /// utile, comme le fait signtool : la charge utile doit rester lisible, et la copie de
    /// maintenance ne doit garder ni la charge utile ni une signature devenue invalide.
    #[test]
    fn signed_windows_setup_keeps_payload_readable() {
        let t = tempfile::tempdir().unwrap();
        let mut pe = vec![0u8; 512];
        pe[0..2].copy_from_slice(b"MZ");
        pe[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        pe[0x80..0x84].copy_from_slice(b"PE\0\0");
        let opt = 0x80 + 24;
        pe[opt..opt + 2].copy_from_slice(&[0x0b, 0x02]);
        pe[opt + 108..opt + 112].copy_from_slice(&16u32.to_le_bytes());
        let stub = t.path().join("stub.exe");
        std::fs::write(&stub, &pe).unwrap();
        let src = t.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join(platform::main_exe_name()), "halcyon").unwrap();
        let out = t.path().join("setup.exe");
        let m = Manifest { product: "Halcyon".into(), version: "0.4.1".into(), platform: "test".into(), main: platform::main_exe_name().into(), min_glibc: None, files: vec![] };
        crate::payload::pack(&stub, &src, m, &out).unwrap();
        // Signature : remplissage à 8 octets puis table de certificats.
        let mut bytes = std::fs::read(&out).unwrap();
        while !bytes.len().is_multiple_of(8) {
            bytes.push(0);
        }
        let cert_at = bytes.len();
        bytes.extend_from_slice(&[0xAB; 40]);
        let entry = opt + 112 + 4 * 8;
        bytes[entry..entry + 4].copy_from_slice(&(cert_at as u32).to_le_bytes());
        bytes[entry + 4..entry + 8].copy_from_slice(&40u32.to_le_bytes());
        std::fs::write(&out, &bytes).unwrap();
        let p = Payload::from_exe(&out).unwrap().expect("charge utile lue malgré la signature");
        assert_eq!(p.manifest.version, "0.4.1");
        let stub_copy = crate::payload::stub_bytes(&bytes);
        assert_eq!(stub_copy.len(), 512);
        assert_eq!(&stub_copy[entry..entry + 8], &[0u8; 8], "signature retirée de l'en-tête");
    }
}

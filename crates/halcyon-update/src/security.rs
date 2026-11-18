//! Authenticité (Ed25519), intégrité (SHA-256), politique de version (anti-retour) et
//! contrôle de la structure des paquets avant toute extraction.
//!
//! Les informations de GitHub (nom de la publication, marque « pre-release », taille des
//! fichiers) ne prouvent rien : seule la signature du manifeste par la clé de l'auteur fait
//! foi, et l'empreinte du paquet est celle du manifeste signé.

use crate::core::PackageFormat;
use crate::version::Version;
use base64::Engine;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;

pub use ed25519_compact::PublicKey;

/// Clés publiques intégrées au logiciel : fichier `keys/release.pub` du dépôt (une clé
/// base64 par ligne, `#` pour les commentaires ; deux clés pendant une rotation) et, en
/// complément, variable `HALCYON_UPDATE_PUBKEY` fixée à la compilation.
const KEY_FILE: &str = include_str!("../keys/release.pub");
const KEY_ENV: Option<&str> = option_env!("HALCYON_UPDATE_PUBKEY");

pub fn parse_keys(text: &str) -> Vec<PublicKey> {
    let b64 = base64::engine::general_purpose::STANDARD;
    text.split(['\n', ','])
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| b64.decode(l).ok())
        .filter_map(|b| PublicKey::from_slice(&b).ok())
        .collect()
}

/// Clés de confiance de cette compilation (vide : installation automatique impossible).
pub fn trusted_keys() -> Vec<PublicKey> {
    let mut keys = parse_keys(KEY_FILE);
    for k in parse_keys(KEY_ENV.unwrap_or("")) {
        if !keys.contains(&k) {
            keys.push(k);
        }
    }
    #[cfg(feature = "fault-injection")]
    if let Ok(extra) = std::env::var("HUS_TEST_PUBKEY") {
        keys.extend(parse_keys(&extra));
    }
    keys
}

/// Empreinte courte d'une clé (affichage, journal).
pub fn fingerprint(key: &PublicKey) -> String {
    hex(&Sha256::digest(key.as_ref())[..8])
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Vérifie une signature Ed25519 (base64) de `message` par l'une des clés `keys`.
pub fn verify_signature(keys: &[PublicKey], message: &[u8], signature_b64: &str) -> Result<(), String> {
    if keys.is_empty() {
        return Err(tr!("cette compilation de Halcyon n'intègre aucune clé de signature : installation automatique impossible"));
    }
    let b64 = base64::engine::general_purpose::STANDARD;
    let sig = b64.decode(signature_b64.trim()).map_err(|_| tr!("signature illisible").to_string())?;
    let sig = ed25519_compact::Signature::from_slice(&sig).map_err(|_| tr!("signature invalide"))?;
    if keys.iter().any(|k| k.verify(message, &sig).is_ok()) {
        Ok(())
    } else {
        Err(tr!("signature incorrecte : cette version n'a pas été publiée par l'auteur de Halcyon"))
    }
}

/// Clé publique base64 seule (compatibilité avec la vérification de `SHA256SUMS`).
pub fn verify_signature_b64(pubkey_b64: &str, message: &[u8], signature_b64: &str) -> Result<(), String> {
    let keys = parse_keys(pubkey_b64);
    if keys.is_empty() {
        return Err(tr!("clé publique de signature invalide"));
    }
    verify_signature(&keys, message, signature_b64)
}

/// Empreinte SHA-256 d'un fichier (lecture en continu).
pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex(&h.finalize()))
}

pub fn sha256_bytes(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

/// Une version publiée peut-elle remplacer l'installation ? Jamais de version inférieure ou
/// égale (retour vers une version vulnérable), ni inférieure au plancher de sécurité déjà
/// accepté par cette installation.
pub fn check_version_policy(candidate: &Version, installed: &Version, floor: Option<&Version>) -> Result<(), String> {
    if candidate <= installed {
        return Err(tr!("{candidate} n'est pas plus récente que la version installée ({installed}) : refusée", candidate = candidate, installed = installed));
    }
    if let Some(f) = floor {
        if candidate < f {
            return Err(tr!("{candidate} est inférieure à la version minimale de sécurité ({f}) : refusée", candidate = candidate, f = f));
        }
    }
    Ok(())
}

/// Fichiers obligatoires à la racine d'un paquet Windows.
pub const WINDOWS_MAIN: &str = "Halcyon.exe";
pub const WINDOWS_UPDATER: &str = "halcyon-updater.exe";
const MAX_ENTRIES: usize = 10_000;
const MAX_UNPACKED: u64 = 2 * 1024 * 1024 * 1024;

/// Contrôle de structure, avant toute extraction : ZIP (chemins sûrs, ni lien ni dossier
/// réservé, taille décompressée et taux de compression bornés, fichiers principaux
/// présents) ou AppImage (ELF x86-64, signature AppImage type 2, image SquashFS).
pub fn check_package(path: &Path, format: PackageFormat) -> Result<(), String> {
    match format {
        PackageFormat::Zip => check_zip(path).map(|_| ()),
        PackageFormat::Appimage => check_appimage(path),
    }
}

/// Chemin relatif sûr : séparateurs `/`, ni absolu, ni `..`, ni lecteur, ni dossier réservé.
pub fn safe_relative(name: &str) -> Option<String> {
    let n = name.replace('\\', "/");
    if n.is_empty() || n.starts_with('/') || n.contains(':') || n.contains('\0') {
        return None;
    }
    let parts: Vec<&str> = n.trim_end_matches('/').split('/').collect();
    if parts.iter().any(|p| p.is_empty() || *p == "." || *p == "..") {
        return None;
    }
    // Jamais de données utilisateur (mode portable) ni d'état de HUS dans un paquet.
    let first = parts[0].to_ascii_lowercase();
    if first == ".hus" || first == "data" || first == "portable.txt" || first == "install.json" {
        return None;
    }
    Some(parts.join("/"))
}

/// Entrées (fichiers) d'un paquet ZIP valide.
pub fn check_zip(path: &Path) -> Result<Vec<String>, String> {
    let file = std::fs::File::open(path).map_err(|e| tr!("paquet : {e}", e = e))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| tr!("archive ZIP invalide : {e}", e = e))?;
    if zip.len() > MAX_ENTRIES {
        return Err(tr!("archive ZIP : trop de fichiers"));
    }
    let mut files = Vec::new();
    let mut total = 0u64;
    for i in 0..zip.len() {
        let e = zip.by_index_raw(i).map_err(|e| tr!("archive ZIP : {e}", e = e))?;
        let name = e.name().map_err(|e| tr!("archive ZIP : {e}", e = e))?.to_string();
        let rel = safe_relative(&name).ok_or_else(|| tr!("archive ZIP : chemin refusé « {name} »", name = name))?;
        if e.is_symlink() {
            return Err(tr!("archive ZIP : lien symbolique refusé « {name} »", name = name));
        }
        if e.is_dir() {
            continue;
        }
        total = total.saturating_add(e.size());
        if total > MAX_UNPACKED {
            return Err(tr!("archive ZIP : taille décompressée excessive"));
        }
        if e.size() > 1024 * 1024 && e.size() / e.compressed_size().max(1) > 200 {
            return Err(tr!("archive ZIP : taux de compression anormal pour « {name} »", name = name));
        }
        if files.iter().any(|f: &String| f.eq_ignore_ascii_case(&rel)) {
            return Err(tr!("archive ZIP : « {name} » en double", name = name));
        }
        files.push(rel);
    }
    for required in [WINDOWS_MAIN, WINDOWS_UPDATER] {
        if !files.iter().any(|f| f == required) {
            return Err(tr!("archive ZIP : {required} absent", required = required));
        }
    }
    Ok(files)
}

fn u16le(b: &[u8], at: usize) -> u64 {
    u16::from_le_bytes([b[at], b[at + 1]]) as u64
}

fn u64le(b: &[u8], at: usize) -> u64 {
    let mut a = [0u8; 8];
    a.copy_from_slice(&b[at..at + 8]);
    u64::from_le_bytes(a)
}

pub fn check_appimage(path: &Path) -> Result<(), String> {
    let mut f = std::fs::File::open(path).map_err(|e| tr!("AppImage : {e}", e = e))?;
    let len = f.metadata().map_err(|e| e.to_string())?.len();
    let mut head = [0u8; 64];
    f.read_exact(&mut head).map_err(|_| tr!("AppImage : fichier trop court"))?;
    if &head[0..4] != b"\x7fELF" || head[4] != 2 || head[5] != 1 {
        return Err(tr!("AppImage : pas un exécutable ELF 64 bits"));
    }
    if &head[8..11] != b"AI\x02" {
        return Err(tr!("AppImage : signature AppImage type 2 absente"));
    }
    if u16le(&head, 18) != 0x3e {
        return Err(tr!("AppImage : architecture différente de x86-64"));
    }
    // L'image SquashFS commence juste après l'exécutable (fin de la table des sections).
    let end = u64le(&head, 0x28).saturating_add(u16le(&head, 0x3a) * u16le(&head, 0x3c));
    if end == 0 || end + 4 > len {
        return Err(tr!("AppImage : image du système de fichiers absente"));
    }
    use std::io::{Seek, SeekFrom};
    let mut magic = [0u8; 4];
    f.seek(SeekFrom::Start(end)).and_then(|_| f.read_exact(&mut magic)).map_err(|e| tr!("AppImage : {e}", e = e))?;
    if &magic != b"hsqs" {
        return Err(tr!("AppImage : image SquashFS introuvable"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn signatures_and_keys() {
        let kp = ed25519_compact::KeyPair::generate();
        let b64 = base64::engine::general_purpose::STANDARD;
        let msg = br#"{"version":"1.1.0"}"#;
        let sig = b64.encode(kp.sk.sign(msg, None).as_ref());
        let keys = parse_keys(&format!("# clé de test\n{}\n", b64.encode(kp.pk.as_ref())));
        assert_eq!(keys.len(), 1);
        assert!(verify_signature(&keys, msg, &sig).is_ok());
        assert!(verify_signature(&keys, br#"{"version":"9.9.9"}"#, &sig).is_err(), "manifeste falsifié");
        let other = ed25519_compact::KeyPair::generate();
        assert!(verify_signature(&[other.pk], msg, &sig).is_err(), "autre clé");
        // Rotation : l'ancienne et la nouvelle clé sont acceptées.
        assert!(verify_signature(&[other.pk, kp.pk], msg, &sig).is_ok());
        assert!(verify_signature(&[], msg, &sig).is_err(), "aucune clé : refus");
        assert!(verify_signature(&keys, msg, "!!").is_err());
        assert!(parse_keys("# rien\n\npas-une-clé").is_empty());
        assert_eq!(fingerprint(&kp.pk).len(), 16);
    }

    #[test]
    fn version_policy_blocks_downgrades() {
        let v = |s: &str| s.parse::<Version>().unwrap();
        assert!(check_version_policy(&v("1.1.0"), &v("1.0.0"), None).is_ok());
        assert!(check_version_policy(&v("1.0.0"), &v("1.0.0"), None).is_err(), "même version");
        assert!(check_version_policy(&v("0.9.0"), &v("1.0.0"), None).is_err(), "retour arrière");
        assert!(check_version_policy(&v("1.0.1"), &v("1.0.0"), Some(&v("1.0.2"))).is_err(), "sous le plancher");
    }

    fn zip_with(entries: &[(&str, &[u8])]) -> tempfile::NamedTempFile {
        let f = tempfile::NamedTempFile::new().unwrap();
        let mut w = zip::ZipWriter::new(f.reopen().unwrap());
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (n, c) in entries {
            w.start_file(*n, opts).unwrap();
            w.write_all(c).unwrap();
        }
        w.finish().unwrap();
        f
    }

    #[test]
    fn zip_structure_is_checked() {
        let ok = zip_with(&[("Halcyon.exe", b"MZ"), ("halcyon-updater.exe", b"MZ"), ("docs/GUIDE.md", b"g")]);
        assert_eq!(check_zip(ok.path()).unwrap().len(), 3);
        for (bad, why) in [
            (vec![("Halcyon.exe", &b"MZ"[..])], "updater absent"),
            (vec![("Halcyon.exe", b"MZ"), ("halcyon-updater.exe", b"MZ"), ("../evil.dll", b"x")], "remontée de dossier"),
            (vec![("Halcyon.exe", b"MZ"), ("halcyon-updater.exe", b"MZ"), ("data/halcyon.db", b"x")], "données utilisateur"),
            (vec![("Halcyon.exe", b"MZ"), ("halcyon-updater.exe", b"MZ"), ("C:/Windows/x.dll", b"x")], "chemin absolu"),
        ] {
            let z = zip_with(&bad);
            assert!(check_zip(z.path()).is_err(), "{why}");
        }
        let bomb = vec![0u8; 4 * 1024 * 1024];
        let z = zip_with(&[("Halcyon.exe", b"MZ"), ("halcyon-updater.exe", b"MZ"), ("bomb.bin", &bomb)]);
        assert!(check_zip(z.path()).is_err(), "taux de compression anormal");
        let not_zip = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(not_zip.path(), b"pas une archive").unwrap();
        assert!(check_package(not_zip.path(), PackageFormat::Zip).is_err());
    }

    /// AppImage minimale : en-tête ELF 64 bits x86-64, signature « AI\x02 », table des
    /// sections vide, image SquashFS juste après.
    pub(crate) fn fake_appimage(magic: &[u8; 3]) -> Vec<u8> {
        let mut b = vec![0u8; 64];
        b[0..4].copy_from_slice(b"\x7fELF");
        b[4] = 2;
        b[5] = 1;
        b[8..11].copy_from_slice(magic);
        b[18] = 0x3e;
        b[0x28..0x30].copy_from_slice(&64u64.to_le_bytes());
        b.extend_from_slice(b"hsqs");
        b.extend_from_slice(&[0u8; 60]);
        b
    }

    #[test]
    fn appimage_format_is_checked() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("a.AppImage");
        std::fs::write(&p, fake_appimage(b"AI\x02")).unwrap();
        assert!(check_appimage(&p).is_ok());
        std::fs::write(&p, fake_appimage(b"AI\x01")).unwrap();
        assert!(check_appimage(&p).is_err(), "AppImage type 1");
        let mut no_fs = fake_appimage(b"AI\x02");
        no_fs[64..68].copy_from_slice(b"xxxx");
        std::fs::write(&p, no_fs).unwrap();
        assert!(check_appimage(&p).is_err(), "sans SquashFS");
        std::fs::write(&p, b"#!/bin/sh\necho pirate\n").unwrap();
        assert!(check_appimage(&p).is_err(), "script");
    }
}

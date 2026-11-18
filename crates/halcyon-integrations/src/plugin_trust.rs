//! Vérification des extensions (1.0.0).
//!
//! - **Empreinte du contenu** : SHA-256 de la liste triée « empreinte  chemin » de tous les
//!   fichiers de l'extension (hors `plugin.sig`). Elle identifie exactement ce qui a été
//!   installé ; une modification après coup est détectée.
//! - **Signature facultative de l'auteur** : `plugin.sig` (JSON `{"key", "signature"}`,
//!   Ed25519 sur l'empreinte), produit par
//!   `cargo run -p halcyon-integrations --example release_sign -- sign-plugin <clé> <dossier>`.
//! - **Épinglage de l'auteur** : une extension installée signée ne peut être mise à jour que
//!   par une version signée par la **même** clé (une mise à jour non signée ou signée par
//!   quelqu'un d'autre est refusée). Une extension non signée est acceptée, et présentée
//!   comme telle.

use base64::Engine;
use std::path::Path;

pub const SIGNATURE_FILE: &str = "plugin.sig";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Trust {
    Unsigned { digest: String },
    /// `key` : clé publique de l'auteur (base64).
    Signed { key: String, digest: String },
}

impl Trust {
    pub fn digest(&self) -> &str {
        match self {
            Trust::Unsigned { digest } | Trust::Signed { digest, .. } => digest,
        }
    }

    pub fn key(&self) -> Option<&str> {
        match self {
            Trust::Signed { key, .. } => Some(key),
            Trust::Unsigned { .. } => None,
        }
    }

    /// Texte affiché : « Signée (clé AB12…EF) » ou « Non signée ».
    pub fn label(&self) -> String {
        match self {
            Trust::Unsigned { digest } => tr!("Non signée · empreinte {v}", v = &digest[..12.min(digest.len())]),
            Trust::Signed { key, .. } => tr!("Signée par l'auteur · clé {key}", key = fingerprint(key)),
        }
    }
}

/// Empreinte courte d'une clé publique (16 caractères hexadécimaux du SHA-256).
pub fn fingerprint(key_b64: &str) -> String {
    let h = crate::archive::sha256_bytes(key_b64.trim().as_bytes());
    h[..16].to_uppercase()
}

/// Empreinte canonique du contenu d'une extension.
pub fn content_digest(dir: &Path) -> Result<String, String> {
    let mut lines = Vec::new();
    // `walk_files` renvoie des chemins relatifs.
    for rel_path in crate::archive::walk_files(dir, 5000).map_err(|e| e.to_string())? {
        let rel = rel_path.to_string_lossy().replace('\\', "/");
        if rel == SIGNATURE_FILE {
            continue;
        }
        lines.push(format!("{}  {rel}", crate::archive::sha256_file(&dir.join(&rel_path)).map_err(|e| e.to_string())?));
    }
    lines.sort();
    Ok(crate::archive::sha256_bytes(lines.join("\n").as_bytes()))
}

/// Vérifie l'extension : `Err` si une signature est présente mais fausse (contenu modifié).
pub fn verify(dir: &Path) -> Result<Trust, String> {
    let digest = content_digest(dir)?;
    let sig_path = dir.join(SIGNATURE_FILE);
    if !sig_path.is_file() {
        return Ok(Trust::Unsigned { digest });
    }
    let text = std::fs::read_to_string(&sig_path).map_err(|e| tr!("{SIGNATURE_FILE} illisible : {e}", SIGNATURE_FILE = SIGNATURE_FILE, e = e))?;
    let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| tr!("{file} invalide : {e}", file = SIGNATURE_FILE, e = e))?;
    let key = v["key"].as_str().ok_or(tr!("{SIGNATURE_FILE} : clé absente", SIGNATURE_FILE = SIGNATURE_FILE))?.trim().to_string();
    let sig = v["signature"].as_str().ok_or(tr!("{SIGNATURE_FILE} : signature absente", SIGNATURE_FILE = SIGNATURE_FILE))?;
    halcyon_update::security::verify_signature_b64(&key, digest.as_bytes(), sig).map_err(|_| tr!("signature de l'extension incorrecte : contenu modifié depuis sa publication"))?;
    Ok(Trust::Signed { key, digest })
}

/// Règle d'épinglage pour remplacer `old` par `new`.
pub fn check_replacement(old: Option<&Trust>, new: &Trust) -> Result<(), String> {
    match (old.and_then(Trust::key), new.key()) {
        (Some(a), Some(b)) if a != b => Err(tr!("mise à jour refusée : signée par une autre clé ({b}) que la version installée ({a})", b = fingerprint(b), a = fingerprint(a))),
        (Some(a), None) => Err(tr!("mise à jour refusée : la version installée est signée (clé {a}), celle-ci ne l'est pas", a = fingerprint(a))),
        _ => Ok(()),
    }
}

/// Signe une extension (outil de l'auteur) : écrit `plugin.sig`.
pub fn sign(dir: &Path, secret_key: &[u8]) -> Result<String, String> {
    let b64 = base64::engine::general_purpose::STANDARD;
    let sk = ed25519_compact::SecretKey::from_slice(secret_key).map_err(|_| tr!("clé privée invalide"))?;
    let digest = content_digest(dir)?;
    let sig = sk.sign(digest.as_bytes(), None);
    let key = b64.encode(sk.public_key().as_ref());
    let v = serde_json::json!({ "key": key, "signature": b64.encode(sig.as_ref()) });
    std::fs::write(dir.join(SIGNATURE_FILE), serde_json::to_string_pretty(&v).unwrap_or_default()).map_err(|e| e.to_string())?;
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plugin(dir: &Path) {
        std::fs::create_dir_all(dir.join("themes")).unwrap();
        std::fs::write(dir.join("plugin.json"), r#"{"id":"x","name":"X","version":"1.0"}"#).unwrap();
        std::fs::write(dir.join("themes/dark.json"), "{}").unwrap();
    }

    #[test]
    fn unsigned_signed_tampered() {
        let d = tempfile::tempdir().unwrap();
        plugin(d.path());
        let t = verify(d.path()).unwrap();
        assert!(matches!(t, Trust::Unsigned { .. }));
        let digest = t.digest().to_string();
        assert_eq!(content_digest(d.path()).unwrap(), digest, "stable");

        let kp = ed25519_compact::KeyPair::generate();
        let key = sign(d.path(), kp.sk.as_ref()).unwrap();
        let t = verify(d.path()).unwrap();
        assert_eq!(t, Trust::Signed { key: key.clone(), digest: digest.clone() }, "la signature n'entre pas dans l'empreinte");
        assert!(t.label().starts_with("Signée par l'auteur · clé "));

        std::fs::write(d.path().join("themes/dark.json"), "{\"bg\":1}").unwrap();
        assert!(verify(d.path()).unwrap_err().contains("contenu modifié"));
    }

    #[test]
    fn pinning_rules() {
        let a = Trust::Signed { key: "A".into(), digest: "1".into() };
        let b = Trust::Signed { key: "B".into(), digest: "2".into() };
        let u = Trust::Unsigned { digest: "3".into() };
        assert!(check_replacement(None, &u).is_ok());
        assert!(check_replacement(Some(&u), &a).is_ok(), "première signature acceptée");
        assert!(check_replacement(Some(&a), &a.clone()).is_ok());
        assert!(check_replacement(Some(&a), &b).is_err());
        assert!(check_replacement(Some(&a), &u).is_err());
    }
}

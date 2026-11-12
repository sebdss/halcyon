//! Accès HTTPS (Mojang, Microsoft, Modrinth, CurseForge, Fabric, Forge…).
//!
//! Toutes les réponses sont bornées en taille ; les fichiers sont écrits dans un fichier
//! temporaire, vérifiés par empreinte, puis renommés : un fichier présent est complet.

use sha1::Digest as _;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Taille maximale d'une réponse JSON.
const MAX_JSON: u64 = 64 * 1024 * 1024;
/// Taille maximale d'un fichier téléchargé (les plus gros sont les runtimes Java, ~50 Mo).
const MAX_FILE: u64 = 1024 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    /// Réseau indisponible ou interrompu (réessayer plus tard).
    Network(String),
    /// Réponse HTTP d'erreur (code, corps éventuel).
    Http(u16, String),
    /// Contenu inattendu ou fichier corrompu.
    Invalid(String),
    /// Écriture sur le disque impossible.
    Disk(String),
    Cancelled,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network(e) => write!(f, "réseau : {e}"),
            Self::Http(code, body) if body.is_empty() => write!(f, "HTTP {code}"),
            Self::Http(code, body) => write!(f, "HTTP {code} : {}", body.chars().take(300).collect::<String>()),
            Self::Invalid(e) => write!(f, "{e}"),
            Self::Disk(e) => write!(f, "disque : {e}"),
            Self::Cancelled => write!(f, "annulé"),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

fn classify(e: ureq::Error) -> Error {
    match e {
        ureq::Error::StatusCode(code) => Error::Http(code, String::new()),
        other => Error::Network(other.to_string()),
    }
}

#[derive(Clone)]
pub struct Http {
    agent: ureq::Agent,
}

impl Default for Http {
    fn default() -> Self {
        Self::new()
    }
}

impl Http {
    pub fn new() -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(15)))
            .timeout_recv_body(Some(Duration::from_secs(120)))
            .http_status_as_error(false)
            .user_agent(concat!("Halcyon/", env!("CARGO_PKG_VERSION"), " (github.com/sebi/halcyon)"))
            .build();
        Self { agent: ureq::Agent::new_with_config(config) }
    }

    fn read_body(mut resp: ureq::http::Response<ureq::Body>) -> Result<Vec<u8>> {
        let status = resp.status().as_u16();
        let bytes = resp.body_mut().with_config().limit(MAX_JSON).read_to_vec().map_err(classify)?;
        if status >= 400 {
            return Err(Error::Http(status, String::from_utf8_lossy(&bytes).trim().to_string()));
        }
        Ok(bytes)
    }

    fn parse(bytes: &[u8], url: &str) -> Result<serde_json::Value> {
        serde_json::from_slice(bytes).map_err(|e| Error::Invalid(format!("{url} : JSON invalide ({e})")))
    }

    /// GET renvoyant du JSON ; `headers` : en-têtes supplémentaires (clés d'API).
    pub fn get_json(&self, url: &str, headers: &[(&str, &str)]) -> Result<serde_json::Value> {
        let mut req = self.agent.get(url).header("Accept", "application/json");
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        let bytes = Self::read_body(req.call().map_err(classify)?)?;
        Self::parse(&bytes, url)
    }

    /// GET renvoyant le corps brut (fichiers XML de Maven…).
    pub fn get_bytes(&self, url: &str) -> Result<Vec<u8>> {
        Self::read_body(self.agent.get(url).call().map_err(classify)?)
    }

    /// POST JSON → JSON.
    pub fn post_json(&self, url: &str, body: &serde_json::Value, headers: &[(&str, &str)]) -> Result<serde_json::Value> {
        let mut req = self.agent.post(url).header("Accept", "application/json").header("Content-Type", "application/json");
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        let bytes = Self::read_body(req.send(body.to_string()).map_err(classify)?)?;
        Self::parse(&bytes, url)
    }

    /// POST de formulaire (OAuth Microsoft) → JSON. Le corps d'erreur est renvoyé tel quel
    /// pour lire les codes OAuth (`authorization_pending`…).
    pub fn post_form(&self, url: &str, form: &[(&str, &str)]) -> Result<serde_json::Value> {
        let body = form.iter().map(|(k, v)| format!("{}={}", encode(k), encode(v))).collect::<Vec<_>>().join("&");
        let resp = self
            .agent
            .post(url)
            .header("Accept", "application/json")
            .header("Content-Type", "application/x-www-form-urlencoded")
            .send(body)
            .map_err(classify)?;
        let status = resp.status().as_u16();
        let mut resp = resp;
        let bytes = resp.body_mut().with_config().limit(MAX_JSON).read_to_vec().map_err(classify)?;
        let json = Self::parse(&bytes, url);
        match (status, json) {
            (s, Ok(v)) if s < 400 => Ok(v),
            (s, Ok(v)) => Err(Error::Http(s, v.to_string())),
            (s, Err(_)) => Err(Error::Http(s, String::from_utf8_lossy(&bytes).to_string())),
        }
    }

    /// Télécharge `url` vers `dest`. `sha1` : empreinte attendue (vérifiée avant de garder le
    /// fichier). `progress` reçoit les octets reçus au fil de l'eau.
    pub fn download(&self, url: &str, dest: &Path, sha1: Option<&str>, cancel: &AtomicBool, progress: &mut dyn FnMut(u64)) -> Result<()> {
        if !url.starts_with("https://") {
            return Err(Error::Invalid(format!("adresse non sécurisée refusée : {url}")));
        }
        let resp = self.agent.get(url).call().map_err(classify)?;
        let status = resp.status().as_u16();
        if status >= 400 {
            return Err(Error::Http(status, String::new()));
        }
        if let Some(d) = dest.parent() {
            std::fs::create_dir_all(d).map_err(|e| Error::Disk(format!("{} : {e}", d.display())))?;
        }
        let tmp = dest.with_extension(format!("{}.part", dest.extension().and_then(|e| e.to_str()).unwrap_or("")));
        let mut file = std::fs::File::create(&tmp).map_err(|e| Error::Disk(format!("{} : {e}", tmp.display())))?;
        let mut hasher = sha1::Sha1::new();
        let mut body = resp.into_body();
        let mut reader = body.with_config().limit(MAX_FILE).reader();
        let mut buf = vec![0u8; 64 * 1024];
        let result = (|| {
            loop {
                if cancel.load(Ordering::Relaxed) {
                    return Err(Error::Cancelled);
                }
                let n = reader.read(&mut buf).map_err(|e| Error::Network(e.to_string()))?;
                if n == 0 {
                    break;
                }
                hasher.update(&buf[..n]);
                file.write_all(&buf[..n]).map_err(|e| Error::Disk(e.to_string()))?;
                progress(n as u64);
            }
            file.flush().map_err(|e| Error::Disk(e.to_string()))?;
            Ok(())
        })();
        drop(file);
        if let Err(e) = result {
            let _ = std::fs::remove_file(&tmp);
            return Err(e);
        }
        let got = hex(&hasher.finalize());
        if let Some(want) = sha1 {
            if !want.eq_ignore_ascii_case(&got) {
                let _ = std::fs::remove_file(&tmp);
                return Err(Error::Invalid(format!("{} : empreinte incorrecte (fichier corrompu en route)", file_name(dest))));
            }
        }
        std::fs::rename(&tmp, dest).map_err(|e| Error::Disk(format!("{} : {e}", dest.display())))
    }
}

fn file_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// SHA-1 d'un fichier (vérification des fichiers déjà présents).
pub fn sha1_file(path: &Path) -> std::io::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut h = sha1::Sha1::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex(&h.finalize()))
}

/// SHA-512 d'un fichier (Modrinth).
pub fn sha512_file(path: &Path) -> std::io::Result<String> {
    use sha2::Digest;
    let mut f = std::fs::File::open(path)?;
    let mut h = sha2::Sha512::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex(&h.finalize()))
}

/// Encodage d'un composant d'URL.
pub fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_and_hashes() {
        assert_eq!(encode("a b&c=é"), "a%20b%26c%3D%C3%A9");
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("x");
        std::fs::write(&p, b"abc").unwrap();
        assert_eq!(sha1_file(&p).unwrap(), "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert!(sha512_file(&p).unwrap().starts_with("ddaf35a193617aba"));
    }
}

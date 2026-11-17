//! Téléchargement des visuels manquants (fonction **en ligne**, désactivable).
//!
//! Les images sont écrites dans le cache local puis référencées en base :
//! elles restent disponibles hors ligne ensuite.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

const MAX_BYTES: u64 = 20 * 1024 * 1024;

/// Échec d'un téléchargement : le réseau (à retenter plus tard) ou la ressource (inutile d'insister).
#[derive(Debug)]
pub enum FetchError {
    Network(String),
    Unavailable(String),
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network(e) => write!(f, "{}", tr!("réseau : {e}", e = e)),
            Self::Unavailable(e) => write!(f, "indisponible : {e}"),
        }
    }
}

fn classify(e: ureq::Error) -> FetchError {
    match e {
        ureq::Error::StatusCode(401) | ureq::Error::StatusCode(403) => {
            FetchError::Unavailable(tr!("accès refusé (clé d'API invalide ou profil privé)").into())
        }
        ureq::Error::StatusCode(429) => FetchError::Network(tr!("trop de requêtes, réessai plus tard").into()),
        ureq::Error::StatusCode(code) => FetchError::Unavailable(format!("HTTP {code}")),
        other => FetchError::Network(other.to_string()),
    }
}

pub struct Downloader {
    agent: ureq::Agent,
}

impl Default for Downloader {
    fn default() -> Self {
        Self::new()
    }
}

impl Downloader {
    pub fn new() -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(20)))
            .user_agent(concat!("Halcyon/", env!("CARGO_PKG_VERSION")))
            .build();
        Self { agent: ureq::Agent::new_with_config(config) }
    }

    /// Requête GET renvoyant du JSON.
    pub fn get_json(&self, url: &str) -> Result<serde_json::Value, FetchError> {
        self.get_json_with(url, &[])
    }

    /// Requête GET avec en-têtes (clé d'API transmise dans un en-tête, jamais dans l'URL).
    pub fn get_json_with(&self, url: &str, headers: &[(&str, &str)]) -> Result<serde_json::Value, FetchError> {
        let mut req = self.agent.get(url);
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        let mut resp = req.call().map_err(classify)?;
        let bytes = resp.body_mut().with_config().limit(MAX_BYTES).read_to_vec().map_err(classify)?;
        serde_json::from_slice(&bytes).map_err(|e| FetchError::Unavailable(tr!("JSON invalide : {e}", e = e)))
    }

    /// Télécharge un fichier quelconque (icônes, captures) vers `dest`.
    pub fn fetch_to(&self, url: &str, dest: &Path) -> Result<(), FetchError> {
        let mut resp = self.agent.get(url).call().map_err(classify)?;
        let bytes = resp.body_mut().with_config().limit(MAX_BYTES).read_to_vec().map_err(classify)?;
        let tmp = dest.with_extension("part");
        std::fs::write(&tmp, &bytes).map_err(|e| FetchError::Network(tr!("écriture : {e}", e = e)))?;
        std::fs::rename(&tmp, dest).map_err(|e| FetchError::Network(tr!("écriture : {e}", e = e)))
    }

    /// Essaie chaque URL dans l'ordre ; enregistre la première image valide sous
    /// `dest_stem` + extension déduite du type MIME.
    pub fn fetch_first(&self, urls: &[String], dest_stem: &Path) -> Result<PathBuf, FetchError> {
        let mut last_err = FetchError::Unavailable(tr!("aucune URL").into());
        for url in urls {
            match self.fetch(url, dest_stem) {
                Ok(p) => return Ok(p),
                Err(e @ FetchError::Network(_)) => return Err(e),
                Err(e) => last_err = e,
            }
        }
        Err(last_err)
    }

    fn fetch(&self, url: &str, dest_stem: &Path) -> Result<PathBuf, FetchError> {
        let unavailable = |e: String| FetchError::Unavailable(e);
        let mut resp = self.agent.get(url).call().map_err(classify)?;
        let mime = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_ascii_lowercase();
        let ext = match mime.split(';').next().unwrap_or("").trim() {
            "image/jpeg" | "image/jpg" => "jpg",
            "image/png" => "png",
            "image/webp" => "webp",
            other => return Err(unavailable(tr!("type de contenu inattendu « {other} »", other = other))),
        };
        let bytes = resp.body_mut().with_config().limit(MAX_BYTES).read_to_vec().map_err(classify)?;
        if bytes.len() < 64 {
            return Err(unavailable(tr!("image vide").into()));
        }
        let dest = dest_stem.with_extension(ext);
        let tmp = dest_stem.with_extension("part");
        let disk = |e: std::io::Error| FetchError::Network(tr!("écriture : {e}", e = e));
        let mut f = std::fs::File::create(&tmp).map_err(disk)?;
        f.write_all(&bytes).map_err(disk)?;
        drop(f);
        std::fs::rename(&tmp, &dest).map_err(disk)?;
        Ok(dest)
    }
}

/// Variantes d'URL à essayer pour un visuel (ex. Steam : version 2x puis 1x).
pub fn url_candidates(url: &str) -> Vec<String> {
    let mut v = vec![url.to_string()];
    if url.contains("_2x.") {
        v.push(url.replace("_2x.", "."));
    }
    v
}

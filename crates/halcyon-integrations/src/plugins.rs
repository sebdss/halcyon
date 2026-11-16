//! Extensions déclaratives.
//!
//! Une extension est un dossier contenant un `plugin.json` : elle **ne contient pas de
//! code** et Halcyon n'exécute rien de ce qu'elle fournit. Elle peut ajouter des
//! émulateurs et plateformes, des emplacements de sauvegarde, des thèmes, des dossiers
//! de jeux à analyser, des sources de métadonnées (requêtes HTTP JSON limitées aux
//! domaines déclarés) et des liens utiles. Voir `docs/PLUGINS.md`.
//!
//! Une extension installée est désactivée tant que le joueur ne l'a pas activée après
//! avoir vu ses autorisations.

use crate::artwork::Downloader;
use halcyon_core::util;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io;
use std::path::{Path, PathBuf};

pub const MANIFEST: &str = "plugin.json";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SaveLocations {
    /// Nom du jeu (comparaison insensible à la casse, aux accents et à la ponctuation).
    pub game: String,
    /// Chemins à jetons (`{documents}`, `{locallow}`, `{steam}`, `{install}`…), `{name}` = nom du jeu.
    pub paths: Vec<String>,
}

/// Source de métadonnées : une requête GET renvoyant du JSON, et des pointeurs JSON.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MetadataSource {
    pub name: String,
    /// `{name}` (encodé), `{steam_appid}` disponibles.
    pub url: String,
    /// Champ → pointeur JSON (RFC 6901), ex. `"description": "/results/0/summary"`.
    pub fields: std::collections::BTreeMap<String, String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Link {
    pub label: String,
    /// `{name}` (encodé) disponible.
    pub url: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Provides {
    pub emulators: Vec<Value>,
    pub platforms: Vec<Value>,
    pub save_locations: Vec<SaveLocations>,
    pub themes: Vec<Value>,
    /// Dossiers supplémentaires analysés comme dossiers de jeux (chemins à jetons).
    pub game_folders: Vec<String>,
    pub metadata: Vec<MetadataSource>,
    pub links: Vec<Link>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub homepage: String,
    /// Version minimale de Halcyon.
    pub min_halcyon: String,
    /// `network:<domaine>` (requêtes vers ce domaine), `folders` (dossiers de jeux).
    pub permissions: Vec<String>,
    /// Adresse d'un `plugin.json` publié, pour vérifier les mises à jour.
    pub update_url: String,
    pub provides: Provides,
}

impl Manifest {
    pub fn network_domains(&self) -> Vec<String> {
        self.permissions.iter().filter_map(|p| p.strip_prefix("network:")).map(|d| d.trim().to_lowercase()).collect()
    }

    /// Le domaine de l'URL est-il autorisé par les permissions ?
    pub fn allows_url(&self, url: &str) -> bool {
        let Some(rest) = url.strip_prefix("https://") else { return false };
        let host = rest.split(['/', '?', '#', ':']).next().unwrap_or("").to_lowercase();
        self.network_domains().iter().any(|d| host == *d || host.ends_with(&format!(".{d}")))
    }

    /// Description lisible des autorisations et apports, affichée avant activation.
    pub fn permission_summary(&self) -> Vec<String> {
        let mut out = Vec::new();
        for d in self.network_domains() {
            out.push(format!("Requêtes réseau vers {d}"));
        }
        if self.permissions.iter().any(|p| p == "folders") && !self.provides.game_folders.is_empty() {
            out.push(format!("Analyse de {} dossier(s) de jeux : {}", self.provides.game_folders.len(), self.provides.game_folders.join(", ")));
        }
        let p = &self.provides;
        let mut adds = Vec::new();
        if !p.emulators.is_empty() {
            adds.push(format!("{} émulateur(s)", p.emulators.len()));
        }
        if !p.platforms.is_empty() {
            adds.push(format!("{} plateforme(s)", p.platforms.len()));
        }
        if !p.save_locations.is_empty() {
            adds.push(format!("emplacements de sauvegarde pour {} jeu(x)", p.save_locations.len()));
        }
        if !p.themes.is_empty() {
            adds.push(format!("{} thème(s)", p.themes.len()));
        }
        if !p.metadata.is_empty() {
            adds.push(format!("{} source(s) de métadonnées", p.metadata.len()));
        }
        if !p.links.is_empty() {
            adds.push(format!("{} lien(s) dans les fiches", p.links.len()));
        }
        if !adds.is_empty() {
            out.push(format!("Ajoute : {}", adds.join(", ")));
        }
        out.push("Aucun code n'est exécuté : l'extension ne contient que des données.".into());
        out
    }

    /// Erreurs bloquantes et avertissements de validation.
    pub fn validate(&self, current_version: &str) -> (Vec<String>, Vec<String>) {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        if self.id.is_empty() || !self.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            errors.push("identifiant manquant ou invalide (lettres, chiffres, - et _)".into());
        }
        if self.name.trim().is_empty() {
            errors.push("nom manquant".into());
        }
        if self.version.trim().is_empty() {
            errors.push("version manquante".into());
        }
        if !self.min_halcyon.is_empty() && util::version_newer(&self.min_halcyon, current_version) {
            errors.push(format!("nécessite Halcyon {} ou plus récent", self.min_halcyon));
        }
        for m in &self.provides.metadata {
            if !self.allows_url(&m.url) {
                warnings.push(format!("source « {} » ignorée : domaine non déclaré dans les permissions (HTTPS requis)", m.name));
            }
        }
        for l in &self.provides.links {
            if !l.url.starts_with("https://") {
                warnings.push(format!("lien « {} » ignoré : HTTPS requis", l.label));
            }
        }
        if !self.provides.game_folders.is_empty() && !self.permissions.iter().any(|p| p == "folders") {
            warnings.push("dossiers de jeux ignorés : permission « folders » absente".into());
        }
        (errors, warnings)
    }
}

/// Extension installée.
#[derive(Clone, Debug, PartialEq)]
pub struct Installed {
    pub dir: PathBuf,
    pub manifest: Option<Manifest>,
    /// Fichier illisible ou incompatible.
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    /// Empreinte et signature (1.0.0) ; `None` si le contenu n'a pas pu être lu.
    pub trust: Option<crate::plugin_trust::Trust>,
}

impl Installed {
    pub fn id(&self) -> String {
        self.manifest.as_ref().map(|m| m.id.clone()).unwrap_or_else(|| self.dir.file_name().unwrap_or_default().to_string_lossy().into_owned())
    }

    pub fn usable(&self) -> bool {
        self.manifest.is_some() && self.errors.is_empty()
    }
}

pub fn read_manifest(path: &Path) -> Result<Manifest, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{MANIFEST} illisible : {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("{MANIFEST} invalide : {e}"))
}

/// Extensions présentes dans `root` (activées ou non).
pub fn list(root: &Path, current_version: &str) -> Vec<Installed> {
    let Ok(entries) = std::fs::read_dir(root) else { return vec![] };
    let mut out: Vec<Installed> = entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| {
            let dir = e.path();
            match read_manifest(&dir.join(MANIFEST)) {
                Ok(m) => {
                    let (mut errors, warnings) = m.validate(current_version);
                    // Une extension signée dont le contenu a changé est inutilisable.
                    let trust = match crate::plugin_trust::verify(&dir) {
                        Ok(t) => Some(t),
                        Err(e) => {
                            errors.push(e);
                            None
                        }
                    };
                    Installed { dir, manifest: Some(m), errors, warnings, trust }
                }
                Err(err) => Installed { dir, manifest: None, errors: vec![err], warnings: vec![], trust: None },
            }
        })
        .collect();
    out.sort_by_key(|p| p.id());
    out
}

/// Prépare l'installation depuis un dossier, un `plugin.json` ou une archive `.zip` :
/// le contenu est copié dans un dossier temporaire et validé, sans rien activer.
pub fn stage(src: &Path, staging: &Path, current_version: &str) -> Result<Manifest, String> {
    let _ = std::fs::remove_dir_all(staging);
    std::fs::create_dir_all(staging).map_err(|e| e.to_string())?;
    let result = if src.is_dir() {
        crate::archive::copy_dir(src, staging).map(|_| ())
    } else if src.file_name().is_some_and(|n| n == MANIFEST) {
        std::fs::copy(src, staging.join(MANIFEST)).map(|_| ())
    } else if src.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip") || e.eq_ignore_ascii_case("halcyonplugin")) {
        crate::archive::extract_prefix(src, "", staging).map(|_| ())
    } else {
        Err(io::Error::other("choisissez un dossier, un fichier plugin.json ou une archive .zip / .halcyonplugin"))
    };
    result.map_err(|e| e.to_string())?;
    // Archive contenant un dossier racine unique.
    if !staging.join(MANIFEST).is_file() {
        let entries: Vec<_> = std::fs::read_dir(staging).map_err(|e| e.to_string())?.flatten().collect();
        if entries.len() == 1 && entries[0].path().join(MANIFEST).is_file() {
            let inner = entries[0].path();
            let tmp = staging.with_extension("inner");
            std::fs::rename(&inner, &tmp).map_err(|e| e.to_string())?;
            std::fs::remove_dir_all(staging).map_err(|e| e.to_string())?;
            std::fs::rename(&tmp, staging).map_err(|e| e.to_string())?;
        }
    }
    // Pas d'exécutables : une extension ne contient que des données.
    for f in crate::archive::walk_files(staging, 5000).map_err(|e| e.to_string())? {
        let ext = f.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        if matches!(ext.as_str(), "exe" | "dll" | "so" | "dylib" | "bat" | "cmd" | "ps1" | "sh" | "msi" | "scr" | "com" | "vbs" | "js" | "py") {
            return Err(format!("fichier exécutable refusé dans l'extension : {}", f.display()));
        }
    }
    let m = read_manifest(&staging.join(MANIFEST))?;
    let (errors, _) = m.validate(current_version);
    if let Some(e) = errors.first() {
        return Err(e.clone());
    }
    Ok(m)
}

/// Déplace l'extension préparée dans `root/<id>` (remplace une version précédente).
pub fn install(staging: &Path, root: &Path, id: &str) -> io::Result<PathBuf> {
    std::fs::create_dir_all(root)?;
    let dest = root.join(id);
    if dest.exists() {
        std::fs::remove_dir_all(&dest)?;
    }
    std::fs::rename(staging, &dest).or_else(|_| crate::archive::copy_dir(staging, &dest).map(|_| ()))?;
    let _ = std::fs::remove_dir_all(staging);
    Ok(dest)
}

/// Version publiée à `update_url`, si plus récente que la version installée.
pub fn check_update(dl: &Downloader, m: &Manifest) -> Result<Option<String>, String> {
    if m.update_url.is_empty() {
        return Ok(None);
    }
    if !m.update_url.starts_with("https://") {
        return Err("adresse de mise à jour non sécurisée (HTTPS requis)".into());
    }
    let v = dl.get_json(&m.update_url).map_err(|e| e.to_string())?;
    let latest = v.get("version").and_then(|x| x.as_str()).ok_or("réponse sans numéro de version")?;
    Ok(util::version_newer(latest, &m.version).then(|| latest.to_string()))
}

pub(crate) fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Remplit un modèle d'URL (`{name}`, `{steam_appid}`).
pub fn fill_url(template: &str, name: &str, steam_appid: Option<&str>) -> String {
    template.replace("{name}", &encode(name)).replace("{steam_appid}", steam_appid.unwrap_or(""))
}

/// Interroge une source de métadonnées ; seuls les champs trouvés sont renvoyés.
pub fn fetch_metadata(
    dl: &Downloader,
    m: &Manifest,
    src: &MetadataSource,
    name: &str,
    steam_appid: Option<&str>,
) -> Result<halcyon_core::OnlineMetadata, String> {
    let url = fill_url(&src.url, name, steam_appid);
    if !m.allows_url(&url) {
        return Err("domaine non autorisé".into());
    }
    let v = dl.get_json(&url).map_err(|e| e.to_string())?;
    Ok(extract_metadata(&v, src))
}

pub fn extract_metadata(v: &Value, src: &MetadataSource) -> halcyon_core::OnlineMetadata {
    let get = |k: &str| src.fields.get(k).and_then(|p| v.pointer(p));
    let text = |k: &str| get(k).and_then(|x| x.as_str()).map(|s| util::strip_html(s).trim().to_string()).filter(|s| !s.is_empty());
    let list = |k: &str| -> Vec<String> {
        match get(k) {
            Some(Value::Array(a)) => a
                .iter()
                .filter_map(|x| x.as_str().map(str::to_string).or_else(|| x.get("name").and_then(|n| n.as_str()).map(str::to_string)))
                .collect(),
            Some(Value::String(s)) => s.split(',').map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect(),
            _ => vec![],
        }
    };
    halcyon_core::OnlineMetadata {
        description: text("description"),
        developer: text("developer"),
        publisher: text("publisher"),
        release_date: text("release_date"),
        genres: list("genres"),
        screenshots: list("screenshots").into_iter().filter(|u| u.starts_with("https://")).take(8).collect(),
        tags: list("tags"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn manifest() -> Manifest {
        serde_json::from_value(json!({
            "id": "demo", "name": "Démo", "version": "1.2.0", "min_halcyon": "0.3.0",
            "permissions": ["network:api.example.com", "folders"],
            "provides": {
                "metadata": [
                    {"name": "Exemple", "url": "https://api.example.com/games?q={name}",
                     "fields": {"description": "/results/0/summary", "genres": "/results/0/genres", "developer": "/results/0/studio"}},
                    {"name": "Piège", "url": "https://evil.org/x?q={name}", "fields": {}}
                ],
                "links": [{"label": "Wiki", "url": "https://wiki.example.com/{name}"}, {"label": "HTTP", "url": "http://x"}],
                "game_folders": ["{home}/Jeux itch"]
            }
        }))
        .unwrap()
    }

    #[test]
    fn permissions_restrict_network_access() {
        let m = manifest();
        assert!(m.allows_url("https://api.example.com/games?q=x"));
        assert!(m.allows_url("https://eu.api.example.com/x"));
        assert!(!m.allows_url("http://api.example.com/"), "HTTPS obligatoire");
        assert!(!m.allows_url("https://api.example.com.evil.org/"));
        assert!(!m.allows_url("https://evil.org/"));
        let (errors, warnings) = m.validate("0.3.0");
        assert!(errors.is_empty());
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        let (errors, _) = m.validate("0.2.0");
        assert_eq!(errors.len(), 1, "version trop ancienne");
        assert!(m.permission_summary().iter().any(|s| s.contains("api.example.com")));
        assert_eq!(fill_url("https://a/{name}", "Hadès II", None), "https://a/Had%C3%A8s%20II");
    }

    #[test]
    fn extracts_metadata_by_pointer() {
        let m = manifest();
        let v = json!({"results": [{"summary": "<b>Un</b> jeu", "genres": [{"name": "Rogue-lite"}, "Action"], "studio": "Supergiant"}]});
        let md = extract_metadata(&v, &m.provides.metadata[0]);
        assert_eq!(md.description.as_deref(), Some("Un jeu"));
        assert_eq!(md.genres, ["Rogue-lite", "Action"]);
        assert_eq!(md.developer.as_deref(), Some("Supergiant"));
        assert!(md.publisher.is_none(), "champ non déclaré : rien d'inventé");
    }

    #[test]
    fn staging_rejects_executables_and_invalid_manifests() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join(MANIFEST), serde_json::to_vec(&manifest()).unwrap()).unwrap();
        let staging = tmp.path().join("staging");
        let m = stage(&src, &staging, "0.3.0").unwrap();
        assert_eq!(m.id, "demo");
        let root = tmp.path().join("plugins");
        install(&staging, &root, &m.id).unwrap();
        let listed = list(&root, "0.3.0");
        assert_eq!(listed.len(), 1);
        assert!(listed[0].usable());

        std::fs::write(src.join("payload.dll"), b"MZ").unwrap();
        assert!(stage(&src, &staging, "0.3.0").unwrap_err().contains("exécutable"));
        std::fs::remove_file(src.join("payload.dll")).unwrap();
        std::fs::write(src.join(MANIFEST), b"{\"id\": \"../x\", \"name\": \"\"}").unwrap();
        assert!(stage(&src, &staging, "0.3.0").is_err());
    }
}

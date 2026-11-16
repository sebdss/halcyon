//! Publications GitHub : liste des versions (requête conditionnelle `If-None-Match`, une
//! réponse 304 ne compte pas dans le quota de l'API), filtrage par canal, manifeste signé,
//! choix du paquet de ce système.
//!
//! HTTPS obligatoire, redirections comprises (`https_only`). Les tests utilisent un serveur
//! local en HTTP via [`Net::for_local_tests`], jamais disponible dans l'application.

use crate::core::{candidates, Candidate, Channel, Platform};
use crate::manifest::{self, Checked, Manifest, Package};
use crate::security::{self, PublicKey};
use crate::version::Version;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Dépôt officiel des versions.
pub const OFFICIAL_REPO: &str = "sebdss/halycon";

#[derive(Clone)]
pub struct Net {
    pub(crate) agent: ureq::Agent,
    api_base: String,
}

impl Net {
    pub fn new() -> Net {
        Net::build("https://api.github.com", true)
    }

    /// Serveur local en HTTP (tests d'intégration uniquement).
    #[cfg(any(test, feature = "fault-injection"))]
    pub fn for_local_tests(base: &str) -> Net {
        Net::build(base, false)
    }

    fn build(api_base: &str, https_only: bool) -> Net {
        let config = ureq::Agent::config_builder()
            .https_only(https_only)
            .http_status_as_error(false)
            .max_redirects(5)
            .timeout_resolve(Some(Duration::from_secs(15)))
            .timeout_connect(Some(Duration::from_secs(15)))
            .timeout_recv_response(Some(Duration::from_secs(30)))
            .user_agent(concat!("Halcyon-Update-System/", env!("CARGO_PKG_VERSION")))
            .build();
        Net { agent: ureq::Agent::new_with_config(config), api_base: api_base.trim_end_matches('/').to_string() }
    }

    /// Petit fichier (manifeste, signature), taille bornée.
    pub fn fetch_small(&self, url: &str, limit: u64) -> Result<Vec<u8>, String> {
        let mut resp = self.agent.get(url).call().map_err(|e| net_error(&e))?;
        let status = resp.status().as_u16();
        if status != 200 {
            return Err(format!("{} : réponse HTTP {status}", short(url)));
        }
        resp.body_mut().with_config().limit(limit).read_to_vec().map_err(|e| format!("{} : {e}", short(url)))
    }
}

impl Default for Net {
    fn default() -> Self {
        Net::new()
    }
}

fn short(url: &str) -> &str {
    url.rsplit('/').next().unwrap_or(url)
}

pub(crate) fn net_error(e: &ureq::Error) -> String {
    match e {
        ureq::Error::Timeout(_) => "délai dépassé : serveur injoignable ou connexion trop lente".into(),
        ureq::Error::HostNotFound => "serveur introuvable : vérifiez la connexion à Internet".into(),
        ureq::Error::ConnectionFailed => "connexion impossible : vérifiez la connexion à Internet".into(),
        ureq::Error::RequireHttpsOnly(_) => "adresse non sécurisée refusée (HTTPS obligatoire)".into(),
        ureq::Error::TooManyRedirects => "trop de redirections".into(),
        other => format!("erreur réseau : {other}"),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReleaseInfo {
    pub tag: String,
    pub version: Version,
    pub prerelease: bool,
    pub html_url: String,
    pub assets: Vec<Asset>,
}

impl Candidate for ReleaseInfo {
    fn version(&self) -> &Version {
        &self.version
    }
    fn flagged_prerelease(&self) -> bool {
        self.prerelease
    }
}

impl ReleaseInfo {
    pub fn asset(&self, name: &str) -> Option<&Asset> {
        self.assets.iter().find(|a| a.name == name)
    }
}

/// Publications lisibles d'une réponse `GET /repos/{repo}/releases` : brouillons et tags
/// non SemVer ignorés.
pub fn parse_releases(v: &serde_json::Value) -> Vec<ReleaseInfo> {
    let Some(list) = v.as_array() else { return Vec::new() };
    list.iter()
        .filter(|r| !r.get("draft").and_then(|d| d.as_bool()).unwrap_or(false))
        .filter_map(|r| {
            let tag = r.get("tag_name")?.as_str()?.to_string();
            let version: Version = tag.parse().ok()?;
            Some(ReleaseInfo {
                version,
                prerelease: r.get("prerelease").and_then(|p| p.as_bool()).unwrap_or(false),
                html_url: r.get("html_url").and_then(|u| u.as_str()).unwrap_or_default().to_string(),
                assets: r
                    .get("assets")
                    .and_then(|a| a.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|x| {
                                Some(Asset {
                                    name: x.get("name")?.as_str()?.to_string(),
                                    url: x.get("browser_download_url")?.as_str()?.to_string(),
                                    size: x.get("size").and_then(|s| s.as_u64()).unwrap_or(0),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                tag,
            })
        })
        .collect()
}

#[derive(Serialize, Deserialize, Default)]
struct FeedCache {
    repo: String,
    etag: String,
    body: String,
}

fn valid_repo(repo: &str) -> Result<&str, String> {
    let r = repo.trim().trim_matches('/');
    let ok = r.split('/').count() == 2 && r.split('/').all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b)) && !p.starts_with('.'));
    if ok { Ok(r) } else { Err(format!("dépôt invalide « {repo} » (attendu : propriétaire/nom)")) }
}

/// Liste des publications, avec cache et `If-None-Match` (fichier `cache`).
pub fn fetch_releases(net: &Net, repo: &str, cache: &Path) -> Result<Vec<ReleaseInfo>, String> {
    let repo = valid_repo(repo)?;
    let cached: FeedCache = std::fs::read(cache).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    let url = format!("{}/repos/{repo}/releases?per_page=30", net.api_base);
    let mut req = net.agent.get(&url).header("Accept", "application/vnd.github+json").header("X-GitHub-Api-Version", "2022-11-28");
    let usable_cache = cached.repo == repo && !cached.etag.is_empty() && !cached.body.is_empty();
    if usable_cache {
        req = req.header("If-None-Match", cached.etag.as_str());
    }
    let mut resp = req.call().map_err(|e| net_error(&e))?;
    let status = resp.status().as_u16();
    let header = |name: &str| resp.headers().get(name).and_then(|v| v.to_str().ok()).map(str::to_string);
    let body = match status {
        304 if usable_cache => cached.body,
        200 => {
            let etag = header("etag").unwrap_or_default();
            let body = resp.body_mut().with_config().limit(8 * 1024 * 1024).read_to_string().map_err(|e| format!("liste des versions : {e}"))?;
            if let Some(dir) = cache.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(cache, serde_json::to_vec(&FeedCache { repo: repo.into(), etag, body: body.clone() }).unwrap_or_default());
            body
        }
        403 | 429 if header("x-ratelimit-remaining").as_deref() == Some("0") => {
            let wait = header("x-ratelimit-reset")
                .and_then(|r| r.parse::<u64>().ok())
                .and_then(|reset| reset.checked_sub(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?.as_secs()))
                .map(|s| format!(" (nouvel essai possible dans {} min)", s.div_ceil(60).max(1)))
                .unwrap_or_default();
            return Err(format!("GitHub limite temporairement les requêtes{wait}"));
        }
        403 | 429 => return Err("GitHub refuse temporairement la requête : réessayez plus tard".into()),
        404 => return Err(format!("dépôt « {repo} » introuvable sur GitHub")),
        s if s >= 500 => return Err(format!("GitHub indisponible (HTTP {s}) : réessayez plus tard")),
        s => return Err(format!("réponse inattendue de GitHub (HTTP {s})")),
    };
    let v: serde_json::Value = serde_json::from_str(&body).map_err(|e| format!("réponse de GitHub illisible : {e}"))?;
    Ok(parse_releases(&v))
}

/// Mise à jour proposable, manifeste vérifié.
#[derive(Clone, Debug)]
pub struct Found {
    pub checked: Checked,
    pub package: Package,
    pub package_url: String,
    /// Octets exacts du manifeste et sa signature (revérifiés par l'updater).
    pub manifest_bytes: Vec<u8>,
    pub signature: String,
    pub release_url: String,
    /// Mise à jour de sécurité : la version installée est sous le plancher annoncé.
    pub security: bool,
}

#[derive(Clone, Debug)]
pub enum Outcome {
    /// Rien de plus récent sur ce canal. `note` : explication utile (passage Beta → Stable,
    /// publication sans manifeste HUS…).
    UpToDate { note: Option<String> },
    Available(Box<Found>),
}

/// Paramètres d'une vérification.
pub struct Query<'a> {
    pub repo: &'a str,
    pub channel: Channel,
    pub current: &'a Version,
    pub platform: Platform,
    pub keys: &'a [PublicKey],
    /// Plancher de sécurité déjà accepté par cette installation.
    pub floor: Option<&'a Version>,
    pub cache: PathBuf,
}

/// Vérifie le manifeste d'une publication : signature d'abord, puis cohérence, paquet de
/// ce système et concordance avec les fichiers réellement publiés.
pub fn inspect(net: &Net, release: &ReleaseInfo, q: &Query) -> Result<Option<Found>, String> {
    let (Some(m_asset), Some(s_asset)) = (release.asset(manifest::MANIFEST_NAME), release.asset(manifest::SIGNATURE_NAME)) else {
        return Ok(None);
    };
    let bytes = net.fetch_small(&m_asset.url, manifest::MAX_MANIFEST)?;
    let sig = String::from_utf8(net.fetch_small(&s_asset.url, manifest::MAX_SIGNATURE)?).map_err(|_| "signature illisible".to_string())?;
    security::verify_signature(q.keys, &bytes, &sig).map_err(|e| format!("{} : {e}", release.version))?;
    let checked = Manifest::parse(&bytes)?.check(Some(&release.version), Some(release.prerelease))?;
    let Some(package) = checked.package(q.platform).cloned() else {
        return Err(format!("{} ne propose pas de paquet pour {}", release.version, q.platform.id()));
    };
    let asset = release.asset(&package.name).ok_or_else(|| format!("{} absent de la publication {}", package.name, release.version))?;
    if asset.size != package.size {
        return Err(format!("{} : taille publiée ({}) différente du manifeste signé ({})", package.name, asset.size, package.size));
    }
    security::check_version_policy(&checked.version, q.current, q.floor)?;
    let security = checked.security_floor.as_ref().is_some_and(|f| q.current < f);
    Ok(Some(Found { package_url: asset.url.clone(), manifest_bytes: bytes, signature: sig.trim().to_string(), release_url: release.html_url.clone(), security, checked, package }))
}

/// Recherche une mise à jour sur le canal demandé.
pub fn check(net: &Net, q: &Query) -> Result<Outcome, String> {
    let releases = fetch_releases(net, q.repo, &q.cache)?;
    let mut skipped = Vec::new();
    for r in candidates(&releases, q.channel, q.current) {
        match inspect(net, r, q)? {
            Some(found) => return Ok(Outcome::Available(Box::new(found))),
            None => skipped.push(r.version.to_string()),
        }
    }
    let note = if !skipped.is_empty() {
        Some(format!("Version(s) {} publiée(s) sans manifeste signé : installation manuelle depuis GitHub uniquement", skipped.join(", ")))
    } else if q.channel == Channel::Stable && q.current.is_prerelease() {
        Some(format!("Canal Stable : la prochaine version stable plus récente que {} sera proposée (pas de retour vers une version plus ancienne)", q.current))
    } else {
        None
    };
    Ok(Outcome::UpToDate { note })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_releases_skipping_drafts_and_bad_tags() {
        let v = serde_json::json!([
            {"tag_name": "v1.1.0-beta.1", "prerelease": true, "draft": false, "html_url": "h", "assets": [{"name": "halcyon-manifest.json", "browser_download_url": "https://x/m", "size": 10}]},
            {"tag_name": "v1.2.0", "draft": true, "assets": []},
            {"tag_name": "nightly", "assets": []},
            {"tag_name": "1.0.0", "prerelease": false, "assets": []}
        ]);
        let r = parse_releases(&v);
        assert_eq!(r.len(), 2);
        assert!(r[0].prerelease && r[0].asset("halcyon-manifest.json").is_some());
        assert_eq!(r[1].version, "1.0.0".parse().unwrap());
        assert!(valid_repo("sebdss/halycon").is_ok());
        assert!(valid_repo("a/b/c").is_err() && valid_repo("a b/c").is_err() && valid_repo("../x").is_err());
    }
}

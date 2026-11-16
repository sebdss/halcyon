//! Mise à jour de Halcyon (fonction **en ligne**, désactivable) : compare la dernière
//! version publiée sur GitHub à la version en cours, puis télécharge le paquet adapté
//! au système et vérifie son empreinte SHA-256 avec le fichier `SHA256SUMS` publié
//! avec la version. Sans empreinte vérifiable, rien n'est installé.
//!
//! **Authenticité (1.0.0)** : l'empreinte prouve que le fichier n'a pas été abîmé, pas
//! qu'il vient bien de l'auteur (un compte GitHub compromis peut publier paquet *et*
//! empreintes). Les versions officielles embarquent donc une clé publique Ed25519
//! (`HALCYON_UPDATE_PUBKEY`, fixée à la compilation par la CI) ; `SHA256SUMS` doit alors être
//! accompagné de `SHA256SUMS.sig`, signé par la clé privée de l'auteur (secret de la CI,
//! jamais dans le dépôt). Signature absente ou invalide : rien n'est installé. Une
//! compilation locale sans clé reste limitée à la vérification d'intégrité, et l'indique.

use crate::artwork::{Downloader, FetchError};
use halcyon_core::util::version_newer;
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    pub url: String,
    pub assets: Vec<Asset>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
}

/// Taille maximale d'un paquet téléchargé.
const MAX_PACKAGE: u64 = 400 * 1024 * 1024;

/// Paquet adapté à ce système : installateur Windows ; sous Linux, l'installateur Halcyon
/// si la version en cours a été installée par lui (`managed`), sinon le `.deb` si dpkg est
/// présent, sinon l'installateur ou l'archive.
pub fn pick_asset(assets: &[Asset], windows: bool, has_dpkg: bool, managed: bool) -> Option<&Asset> {
    let lower = |a: &Asset| a.name.to_lowercase();
    if windows {
        return assets
            .iter()
            .find(|a| lower(a).ends_with("-setup.exe"))
            .or_else(|| assets.iter().find(|a| lower(a).ends_with(".exe") || lower(a).ends_with(".msi")));
    }
    let setup = assets.iter().find(|a| lower(a).ends_with("-linux-x86_64-setup"));
    let deb = assets.iter().find(|a| lower(a).ends_with(".deb"));
    let tar = assets.iter().find(|a| lower(a).contains("linux") && lower(a).ends_with(".tar.gz"));
    if managed {
        setup.or(deb).or(tar)
    } else if has_dpkg {
        deb.or(setup).or(tar)
    } else {
        setup.or(tar).or(deb)
    }
}

/// Lit un fichier `SHA256SUMS` (« empreinte  nom » par ligne, nom éventuellement préfixé de `*`).
pub fn parse_sums(text: &str) -> std::collections::HashMap<String, String> {
    text.lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let hash = it.next()?.to_lowercase();
            let name = it.next()?.trim_start_matches('*');
            (hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit())).then(|| (name.to_string(), hash))
        })
        .collect()
}

/// Clé publique de signature des versions (base64, 32 octets), fixée à la compilation.
pub const UPDATE_PUBKEY: Option<&str> = option_env!("HALCYON_UPDATE_PUBKEY");

/// Niveau de vérification appliqué par cette compilation.
pub fn verification_level() -> &'static str {
    match UPDATE_PUBKEY {
        Some(k) if !k.trim().is_empty() => "intégrité (SHA-256) et authenticité (signature Ed25519)",
        _ => "intégrité seule (SHA-256) : compilation sans clé de signature",
    }
}

/// Vérifie la signature Ed25519 (base64) de `message` avec la clé publique `pubkey_b64`.
pub fn verify_signature(pubkey_b64: &str, message: &[u8], signature_b64: &str) -> Result<(), String> {
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD;
    let pk = b64.decode(pubkey_b64.trim()).map_err(|_| "clé publique de signature illisible".to_string())?;
    let sig = b64.decode(signature_b64.trim()).map_err(|_| "signature illisible".to_string())?;
    let pk = ed25519_compact::PublicKey::from_slice(&pk).map_err(|_| "clé publique de signature invalide".to_string())?;
    let sig = ed25519_compact::Signature::from_slice(&sig).map_err(|_| "signature invalide".to_string())?;
    pk.verify(message, &sig).map_err(|_| "signature incorrecte : cette version n'a pas été publiée par l'auteur de Halcyon".to_string())
}

/// Télécharge `asset` dans `dir` et vérifie son empreinte. Le fichier est supprimé si elle ne correspond pas.
pub fn download_verified(release: &Release, asset: &Asset, dir: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let sums = release
        .assets
        .iter()
        .find(|a| matches!(a.name.as_str(), "SHA256SUMS" | "SHA256SUMS.txt" | "sha256sums.txt"))
        .ok_or("cette version ne publie pas de fichier SHA256SUMS : intégrité non vérifiable, téléchargement refusé")?;
    if !asset.url.starts_with("https://") || !sums.url.starts_with("https://") {
        return Err("adresse de téléchargement non sécurisée".into());
    }
    if asset.size > MAX_PACKAGE {
        return Err("paquet anormalement volumineux".into());
    }
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(900)))
        .user_agent(concat!("Halcyon/", env!("CARGO_PKG_VERSION")))
        .build();
    let agent = ureq::Agent::new_with_config(config);
    let text = agent
        .get(&sums.url)
        .call()
        .map_err(|e| format!("SHA256SUMS : {e}"))?
        .body_mut()
        .with_config()
        .limit(1024 * 1024)
        .read_to_string()
        .map_err(|e| format!("SHA256SUMS : {e}"))?;
    match UPDATE_PUBKEY.map(str::trim).filter(|k| !k.is_empty()) {
        Some(key) => {
            let sig_asset = release
                .assets
                .iter()
                .find(|a| matches!(a.name.as_str(), "SHA256SUMS.sig" | "SHA256SUMS.txt.sig"))
                .ok_or("cette version n'est pas signée (SHA256SUMS.sig absent) : téléchargement refusé")?;
            if !sig_asset.url.starts_with("https://") {
                return Err("adresse de téléchargement non sécurisée".into());
            }
            let sig = agent
                .get(&sig_asset.url)
                .call()
                .map_err(|e| format!("SHA256SUMS.sig : {e}"))?
                .body_mut()
                .with_config()
                .limit(4096)
                .read_to_string()
                .map_err(|e| format!("SHA256SUMS.sig : {e}"))?;
            if let Err(e) = verify_signature(key, text.as_bytes(), &sig) {
                halcyon_storage::audit::event("update.rejected", &format!("{} : {e}", release.version));
                return Err(e);
            }
        }
        None => log::warn!("mise à jour : compilation sans clé de signature, vérification d'intégrité seule"),
    }
    let expected = parse_sums(&text).remove(&asset.name).ok_or_else(|| format!("{} absent de SHA256SUMS", asset.name))?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let safe = asset.name.replace(['/', '\\'], "_");
    let dest = dir.join(&safe);
    let part = dir.join(format!("{safe}.part"));
    let mut resp = agent.get(&asset.url).call().map_err(|e| format!("téléchargement : {e}"))?;
    let mut reader = resp.body_mut().with_config().limit(MAX_PACKAGE).reader();
    let mut file = std::fs::File::create(&part).map_err(|e| e.to_string())?;
    std::io::copy(&mut reader, &mut file).map_err(|e| {
        let _ = std::fs::remove_file(&part);
        format!("téléchargement interrompu : {e}")
    })?;
    drop(file);
    let actual = crate::archive::sha256_file(&part).map_err(|e| e.to_string())?;
    if actual != expected {
        let _ = std::fs::remove_file(&part);
        return Err(format!("empreinte incorrecte pour {} : fichier supprimé", asset.name));
    }
    std::fs::rename(&part, &dest).map_err(|e| e.to_string())?;
    halcyon_storage::audit::event("update.verified", &format!("{} {} ({})", release.version, asset.name, verification_level()));
    Ok(dest)
}

/// `Ok(Some(..))` si une version plus récente que `current` est publiée sur `repo` (« propriétaire/nom »).
pub fn check(dl: &Downloader, repo: &str, current: &str) -> Result<Option<Release>, FetchError> {
    let repo = repo.trim().trim_matches('/');
    if repo.split('/').count() != 2 || repo.contains(' ') {
        return Err(FetchError::Unavailable(format!("dépôt invalide « {repo} » (attendu : propriétaire/nom)")));
    }
    let v = dl.get_json(&format!("https://api.github.com/repos/{repo}/releases/latest"))?;
    Ok(parse_release(&v).filter(|r| version_newer(&r.version, current)))
}

pub fn parse_release(v: &Value) -> Option<Release> {
    Some(Release {
        version: v.get("tag_name")?.as_str()?.trim_start_matches(['v', 'V']).to_string(),
        url: v.get("html_url")?.as_str()?.to_string(),
        assets: v
            .get("assets")
            .and_then(|a| a.as_array())
            .map(|list| {
                list.iter()
                    .filter_map(|a| {
                        Some(Asset {
                            name: a.get("name")?.as_str()?.to_string(),
                            url: a.get("browser_download_url")?.as_str()?.to_string(),
                            size: a.get("size").and_then(|s| s.as_u64()).unwrap_or(0),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_github_release() {
        let v = serde_json::json!({"tag_name": "v0.3.0", "html_url": "https://github.com/a/b/releases/tag/v0.3.0"});
        let r = parse_release(&v).unwrap();
        assert_eq!(r.version, "0.3.0");
        assert!(version_newer(&r.version, "0.2.1"));
        assert!(check(&Downloader::new(), "pas un dépôt", "0.1.0").is_err());
        assert!(r.assets.is_empty());
    }

    #[test]
    fn ed25519_signature_is_checked() {
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD;
        let kp = ed25519_compact::KeyPair::generate();
        let msg = b"0123  halcyon-1.0.0-setup.exe\n";
        let sig = b64.encode(kp.sk.sign(msg, None).as_ref());
        let pk = b64.encode(kp.pk.as_ref());
        assert!(verify_signature(&pk, msg, &sig).is_ok());
        assert!(verify_signature(&pk, b"0124  halcyon-1.0.0-setup.exe\n", &sig).is_err(), "SHA256SUMS modifié");
        let other = b64.encode(ed25519_compact::KeyPair::generate().pk.as_ref());
        assert!(verify_signature(&other, msg, &sig).is_err(), "autre clé");
        assert!(verify_signature(&pk, msg, "pas du base64 !").is_err());
    }

    #[test]
    fn picks_package_and_parses_sums() {
        let a = |n: &str| Asset { name: n.into(), url: format!("https://x/{n}"), size: 1 };
        let assets = vec![a("halcyon-0.3.0-setup.exe"), a("halcyon_0.3.0_amd64.deb"), a("halcyon-0.3.0-linux-x86_64.tar.gz"), a("SHA256SUMS")];
        assert_eq!(pick_asset(&assets, true, false, false).unwrap().name, "halcyon-0.3.0-setup.exe");
        assert_eq!(pick_asset(&assets, false, true, false).unwrap().name, "halcyon_0.3.0_amd64.deb");
        assert_eq!(pick_asset(&assets, false, false, false).unwrap().name, "halcyon-0.3.0-linux-x86_64.tar.gz");
        // 0.4.1 : installateur Linux, choisi pour une installation qu'il gère.
        let mut assets = assets;
        assets.push(Asset { name: "halcyon-0.4.1-linux-x86_64-setup".into(), url: "https://x/s".into(), size: 1 });
        assert_eq!(pick_asset(&assets, false, true, true).unwrap().name, "halcyon-0.4.1-linux-x86_64-setup");
        assert_eq!(pick_asset(&assets, false, true, false).unwrap().name, "halcyon_0.3.0_amd64.deb");
        assert_eq!(pick_asset(&assets, false, false, false).unwrap().name, "halcyon-0.4.1-linux-x86_64-setup");
        let h = "a".repeat(64);
        let sums = parse_sums(&format!("{h}  halcyon.deb\n{h} *halcyon.exe\nnope\n"));
        assert_eq!(sums.get("halcyon.deb"), Some(&h));
        assert_eq!(sums.get("halcyon.exe"), Some(&h));
        assert_eq!(sums.len(), 2);
    }
}

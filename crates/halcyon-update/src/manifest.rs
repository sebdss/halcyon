//! Manifeste de publication HUS (`halcyon-manifest.json`), signé par
//! `halcyon-manifest.json.sig` (Ed25519, base64, sur les octets exacts du fichier).
//!
//! Format officiel (schéma 1) : voir `docs/HUS.md`. Les champs inconnus sont ignorés (ajouts
//! compatibles) ; un changement incompatible augmente `schema`, et une version de Halcyon qui
//! ne connaît pas ce schéma refuse le manifeste plutôt que de l'interpréter de travers.

use crate::core::{Channel, PackageFormat, Platform};
use crate::version::Version;
use serde::{Deserialize, Serialize};

pub const SCHEMA: u32 = 1;
pub const PRODUCT: &str = "Halcyon";
pub const MANIFEST_NAME: &str = "halcyon-manifest.json";
pub const SIGNATURE_NAME: &str = "halcyon-manifest.json.sig";
/// Taille maximale acceptée pour le manifeste et sa signature.
pub const MAX_MANIFEST: u64 = 256 * 1024;
pub const MAX_SIGNATURE: u64 = 1024;
/// Taille maximale d'un paquet.
pub const MAX_PACKAGE: u64 = 600 * 1024 * 1024;
const MAX_NOTES: usize = 64 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub schema: u32,
    pub product: String,
    pub channel: Channel,
    pub version: String,
    /// Date de publication, RFC 3339 en UTC (`2026-11-16T14:00:00Z`).
    pub published: String,
    /// Version minimale de l'updater capable d'installer ces paquets.
    pub min_updater: String,
    /// Version minimale de sécurité : une installation plus ancienne doit être mise à jour en
    /// priorité, et aucune version inférieure ne sera plus jamais acceptée par cette installation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub security_floor: Option<String>,
    pub packages: Vec<Package>,
    #[serde(default)]
    pub notes: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Package {
    pub platform: Platform,
    pub format: PackageFormat,
    pub name: String,
    pub size: u64,
    pub sha256: String,
}

/// Manifeste vérifié et cohérent.
#[derive(Clone, Debug, PartialEq)]
pub struct Checked {
    pub manifest: Manifest,
    pub version: Version,
    pub min_updater: Version,
    pub security_floor: Option<Version>,
}

impl Checked {
    pub fn package(&self, platform: Platform) -> Option<&Package> {
        self.manifest.packages.iter().find(|p| p.platform == platform)
    }
}

fn is_sha256(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// `AAAA-MM-JJTHH:MM:SSZ` (secondes fractionnaires tolérées).
fn is_rfc3339_utc(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() < 20 || !s.ends_with('Z') {
        return false;
    }
    let digits = |r: std::ops::Range<usize>| b[r].iter().all(u8::is_ascii_digit);
    digits(0..4) && b[4] == b'-' && digits(5..7) && b[7] == b'-' && digits(8..10) && b[10] == b'T' && digits(11..13) && b[13] == b':' && digits(14..16) && b[16] == b':' && digits(17..19)
}

impl Manifest {
    pub fn parse(bytes: &[u8]) -> Result<Manifest, String> {
        if bytes.len() as u64 > MAX_MANIFEST {
            return Err("manifeste trop volumineux".into());
        }
        serde_json::from_slice(bytes).map_err(|e| format!("manifeste illisible : {e}"))
    }

    /// Cohérence interne, et avec la publication qui le porte (`tag` et marque
    /// « pre-release » de GitHub, si connus). La signature est vérifiée séparément
    /// (`security::verify_manifest`) et **avant** ceci.
    pub fn check(self, tag: Option<&Version>, flagged_prerelease: Option<bool>) -> Result<Checked, String> {
        if self.schema != SCHEMA {
            return Err(format!("format de manifeste {} non pris en charge par cette version (attendu : {SCHEMA})", self.schema));
        }
        if self.product != PRODUCT {
            return Err(format!("manifeste d'un autre produit (« {} »)", self.product));
        }
        let version: Version = self.version.parse().map_err(|e| format!("version du manifeste : {e}"))?;
        if let Some(t) = tag {
            if *t != version {
                return Err(format!("le manifeste annonce {version} mais la publication est {t}"));
            }
        }
        let expected = Channel::of(&version, flagged_prerelease.unwrap_or(false));
        if self.channel != expected {
            return Err(format!("canal incohérent : {version} publiée en « {} » (attendu « {} »)", self.channel.as_str(), expected.as_str()));
        }
        if !is_rfc3339_utc(&self.published) {
            return Err("date de publication invalide".into());
        }
        let min_updater: Version = self.min_updater.parse().map_err(|e| format!("version minimale de l'updater : {e}"))?;
        let security_floor = match &self.security_floor {
            Some(f) => {
                let f: Version = f.parse().map_err(|e| format!("version minimale de sécurité : {e}"))?;
                if f > version {
                    return Err("version minimale de sécurité supérieure à la version publiée".into());
                }
                Some(f)
            }
            None => None,
        };
        if self.packages.is_empty() {
            return Err("aucun paquet dans le manifeste".into());
        }
        for (i, p) in self.packages.iter().enumerate() {
            if self.packages[..i].iter().any(|q| q.platform == p.platform) {
                return Err(format!("deux paquets pour {}", p.platform.id()));
            }
            if p.format != p.platform.format() {
                return Err(format!("format de paquet inattendu pour {}", p.platform.id()));
            }
            if p.name != p.platform.package_name(&version) {
                return Err(format!("nom de paquet inattendu : {} (attendu {})", p.name, p.platform.package_name(&version)));
            }
            if !is_sha256(&p.sha256) {
                return Err(format!("{} : empreinte SHA-256 invalide", p.name));
            }
            if p.size == 0 || p.size > MAX_PACKAGE {
                return Err(format!("{} : taille invalide", p.name));
            }
        }
        if self.notes.len() > MAX_NOTES {
            return Err("notes de version trop longues".into());
        }
        Ok(Checked { manifest: self, version, min_updater, security_floor })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn sample(version: &str) -> Manifest {
        let v: Version = version.parse().unwrap();
        Manifest {
            schema: SCHEMA,
            product: PRODUCT.into(),
            channel: Channel::of(&v, false),
            version: version.into(),
            published: "2026-11-16T14:00:00Z".into(),
            min_updater: "1.0.0-alpha.1".into(),
            security_floor: None,
            packages: vec![
                Package { platform: Platform::WindowsX64, format: PackageFormat::Zip, name: Platform::WindowsX64.package_name(&v), size: 10, sha256: "a".repeat(64) },
                Package { platform: Platform::LinuxX64, format: PackageFormat::Appimage, name: Platform::LinuxX64.package_name(&v), size: 10, sha256: "b".repeat(64) },
            ],
            notes: "Corrections".into(),
        }
    }

    #[test]
    fn consistent_manifest_is_accepted() {
        let tag: Version = "1.1.0-beta.1".parse().unwrap();
        let c = sample("1.1.0-beta.1").check(Some(&tag), Some(true)).unwrap();
        assert_eq!(c.version, tag);
        assert_eq!(c.package(Platform::LinuxX64).unwrap().name, "Halcyon-1.1.0-beta.1-x86_64.AppImage");
        // Aller-retour JSON : le format publié est relu à l'identique ; champ inconnu ignoré.
        let mut json: serde_json::Value = serde_json::to_value(sample("1.0.0")).unwrap();
        json["futur"] = serde_json::json!(true);
        let back = Manifest::parse(json.to_string().as_bytes()).unwrap();
        assert_eq!(back, sample("1.0.0"));
    }

    #[test]
    fn inconsistent_manifests_are_rejected() {
        let tag: Version = "1.0.0".parse().unwrap();
        let reject = |m: Manifest, why: &str| assert!(m.check(Some(&tag), Some(false)).is_err(), "{why}");
        let mut m = sample("1.0.0");
        m.version = "1.0.1".into();
        m.packages.iter_mut().for_each(|p| p.name = p.platform.package_name(&"1.0.1".parse().unwrap()));
        reject(m, "version différente de la publication");
        let mut m = sample("1.0.0");
        m.channel = Channel::Beta;
        reject(m, "canal incohérent");
        let mut m = sample("1.0.0");
        m.product = "Autre".into();
        reject(m, "autre produit");
        let mut m = sample("1.0.0");
        m.schema = 2;
        reject(m, "schéma inconnu");
        let mut m = sample("1.0.0");
        m.packages[0].sha256 = "A".repeat(64);
        reject(m, "empreinte mal formée");
        let mut m = sample("1.0.0");
        m.packages[1].name = "../../evil".into();
        reject(m, "nom de paquet arbitraire");
        let mut m = sample("1.0.0");
        m.packages[1].platform = Platform::WindowsX64;
        reject(m, "plateforme en double");
        let mut m = sample("1.0.0");
        m.security_floor = Some("2.0.0".into());
        reject(m, "plancher de sécurité incohérent");
        let mut m = sample("1.0.0");
        m.published = "hier".into();
        reject(m, "date invalide");
        // Version finale marquée « pre-release » sur GitHub : canal Beta attendu.
        assert!(sample("1.0.0").check(Some(&tag), Some(true)).is_err());
        assert!(Manifest::parse(&vec![b' '; (MAX_MANIFEST + 1) as usize]).is_err());
    }
}

//! Mod loaders : Fabric et Quilt (profils publiés par leurs serveurs de métadonnées), Forge
//! et NeoForge (installateurs officiels, exécutés en mode client avec Java). Chaque loader
//! produit un fichier de version qui hérite de la version vanilla (`inheritsFrom`).

use crate::http::{Error, Http, Result};
use crate::install::{read_json, Event, Layout, Sink};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Loader {
    Vanilla,
    Fabric,
    Quilt,
    Forge,
    NeoForge,
}

impl Loader {
    pub const MODDED: [Loader; 4] = [Self::Fabric, Self::Quilt, Self::Forge, Self::NeoForge];

    pub fn id(self) -> &'static str {
        match self {
            Self::Vanilla => "vanilla",
            Self::Fabric => "fabric",
            Self::Quilt => "quilt",
            Self::Forge => "forge",
            Self::NeoForge => "neoforge",
        }
    }

    pub fn from_id(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "vanilla" | "minecraft" => Some(Self::Vanilla),
            "fabric" | "fabric-loader" => Some(Self::Fabric),
            "quilt" | "quilt-loader" => Some(Self::Quilt),
            "forge" => Some(Self::Forge),
            "neoforge" | "neo-forge" => Some(Self::NeoForge),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Vanilla => "Vanilla",
            Self::Fabric => "Fabric",
            Self::Quilt => "Quilt",
            Self::Forge => "Forge",
            Self::NeoForge => "NeoForge",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LoaderVersion {
    pub version: String,
    /// Stable / recommandée.
    pub stable: bool,
}

const FABRIC_META: &str = "https://meta.fabricmc.net/v2";
const QUILT_META: &str = "https://meta.quiltmc.org/v3";
const FORGE_MAVEN: &str = "https://maven.minecraftforge.net/net/minecraftforge/forge";
const FORGE_PROMOS: &str = "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json";
const NEOFORGE_MAVEN: &str = "https://maven.neoforged.net/releases/net/neoforged/neoforge";

/// Versions de `<version>` d'un fichier `maven-metadata.xml`.
pub fn maven_versions(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(s) = rest.find("<version>") {
        let after = &rest[s + 9..];
        let Some(e) = after.find("</version>") else { break };
        out.push(after[..e].trim().to_string());
        rest = &after[e..];
    }
    out
}

/// Version de Minecraft d'une version de NeoForge : « 21.1.77 » → « 1.21.1 », « 20.4.237 »
/// → « 1.20.4 », « 21.0.10-beta » → « 1.21 », numérotation 2026 « 26.1.0.5 » → « 26.1 ».
pub fn neoforge_mc(v: &str) -> Option<String> {
    let base = v.split('-').next()?;
    let p: Vec<&str> = base.split('.').collect();
    let major: u32 = p.first()?.parse().ok()?;
    if major >= 25 {
        let minor = p.get(1)?;
        return Some(match p.get(2) {
            Some(patch) if p.len() >= 4 && *patch != "0" => format!("{major}.{minor}.{patch}"),
            _ => format!("{major}.{minor}"),
        });
    }
    let minor = p.get(1)?;
    Some(if *minor == "0" { format!("1.{major}") } else { format!("1.{major}.{minor}") })
}

/// Versions du loader compatibles avec `mc`, la plus récente en premier.
pub fn versions(http: &Http, loader: Loader, mc: &str) -> Result<Vec<LoaderVersion>> {
    match loader {
        Loader::Vanilla => Ok(Vec::new()),
        Loader::Fabric | Loader::Quilt => {
            let base = if loader == Loader::Fabric { FABRIC_META } else { QUILT_META };
            let v = http.get_json(&format!("{base}/versions/loader/{}", crate::http::encode(mc)), &[])?;
            Ok(v.as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|e| {
                            let l = e.get("loader")?;
                            let version = l.get("version")?.as_str()?.to_string();
                            // Quilt ne publie pas « stable » : les versions bêta sont marquées dans le numéro.
                            let stable = l.get("stable").and_then(|s| s.as_bool()).unwrap_or(!version.contains("beta"));
                            Some(LoaderVersion { version, stable })
                        })
                        .collect()
                })
                .unwrap_or_default())
        }
        Loader::Forge => {
            let xml = String::from_utf8_lossy(&http.get_bytes(&format!("{FORGE_MAVEN}/maven-metadata.xml"))?).into_owned();
            let promos = http.get_json(FORGE_PROMOS, &[]).ok();
            let rec = promos.as_ref().and_then(|p| p.pointer(&format!("/promos/{mc}-recommended"))).and_then(|v| v.as_str()).map(str::to_string);
            let prefix = format!("{mc}-");
            let mut out: Vec<LoaderVersion> = maven_versions(&xml)
                .into_iter()
                .filter_map(|v| v.strip_prefix(&prefix).map(str::to_string))
                .map(|f| LoaderVersion { stable: rec.as_deref() == Some(f.as_str()), version: f })
                .collect();
            out.reverse();
            out.dedup_by(|a, b| a.version == b.version);
            Ok(out)
        }
        Loader::NeoForge => {
            let xml = String::from_utf8_lossy(&http.get_bytes(&format!("{NEOFORGE_MAVEN}/maven-metadata.xml"))?).into_owned();
            let mut out: Vec<LoaderVersion> = maven_versions(&xml)
                .into_iter()
                .filter(|v| neoforge_mc(v).as_deref() == Some(mc))
                .map(|v| LoaderVersion { stable: !v.contains("beta") && !v.contains("alpha"), version: v })
                .collect();
            out.reverse();
            Ok(out)
        }
    }
}

/// Identifiant de version produit par un loader.
pub fn version_id(loader: Loader, mc: &str, lv: &str) -> String {
    match loader {
        Loader::Vanilla => mc.to_string(),
        Loader::Fabric => format!("fabric-loader-{lv}-{mc}"),
        Loader::Quilt => format!("quilt-loader-{lv}-{mc}"),
        Loader::Forge => format!("{mc}-forge-{lv}"),
        Loader::NeoForge => format!("neoforge-{lv}"),
    }
}

/// Installe le loader (la version vanilla doit être installée avant pour Forge/NeoForge).
/// `java` : exécutable Java utilisé pour les installateurs officiels. Renvoie l'identifiant
/// de la version créée.
#[allow(clippy::too_many_arguments)]
pub fn install(http: &Http, layout: &Layout, loader: Loader, mc: &str, lv: &str, java: Option<&Path>, cancel: &AtomicBool, sink: Sink) -> Result<String> {
    if lv.contains(['/', '\\']) || lv.contains("..") || mc.contains(['/', '\\']) {
        return Err(Error::Invalid(tr!("version refusée").into()));
    }
    match loader {
        Loader::Vanilla => Ok(mc.to_string()),
        Loader::Fabric | Loader::Quilt => {
            sink(Event::Step(tr!("Profil {name} {lv}", name = loader.name(), lv = lv)));
            let base = if loader == Loader::Fabric { FABRIC_META } else { QUILT_META };
            let url = format!("{base}/versions/loader/{}/{}/profile/json", crate::http::encode(mc), crate::http::encode(lv));
            let profile = http.get_json(&url, &[])?;
            let id = profile.get("id").and_then(|i| i.as_str()).ok_or_else(|| Error::Invalid(tr!("profil sans identifiant").into()))?.to_string();
            if id.contains(['/', '\\']) || id.contains("..") {
                return Err(Error::Invalid(tr!("identifiant refusé : {id}", id = id)));
            }
            let path = layout.version_json(&id);
            std::fs::create_dir_all(layout.version_dir(&id)).map_err(|e| Error::Disk(e.to_string()))?;
            std::fs::write(&path, serde_json::to_vec_pretty(&profile).unwrap_or_default()).map_err(|e| Error::Disk(e.to_string()))?;
            Ok(id)
        }
        Loader::Forge | Loader::NeoForge => {
            let java = java.ok_or_else(|| Error::Invalid(tr!("Java est nécessaire pour l'installateur officiel").into()))?;
            let (url, name) = if loader == Loader::Forge {
                let full = format!("{mc}-{lv}");
                (format!("{FORGE_MAVEN}/{full}/forge-{full}-installer.jar"), format!("forge-{full}-installer.jar"))
            } else {
                (format!("{NEOFORGE_MAVEN}/{lv}/neoforge-{lv}-installer.jar"), format!("neoforge-{lv}-installer.jar"))
            };
            sink(Event::Step(tr!("Téléchargement de l'installateur officiel {name}", name = loader.name())));
            // Empreinte publiée par le dépôt Maven officiel.
            let sha1 = http.get_bytes(&format!("{url}.sha1")).ok().map(|b| String::from_utf8_lossy(&b).trim().chars().take(40).collect::<String>());
            let installer = layout.cache().join("installers").join(&name);
            http.download(&url, &installer, sha1.as_deref().filter(|s| s.len() == 40), cancel, &mut |_| {})?;
            // Les installateurs exigent un fichier de profils du launcher officiel.
            let profiles = layout.root.join("launcher_profiles.json");
            if !profiles.is_file() {
                std::fs::write(&profiles, r#"{"profiles":{},"selectedProfile":null}"#).map_err(|e| Error::Disk(e.to_string()))?;
            }
            let before = crate::install::installed(layout);
            sink(Event::Step(tr!("Installateur officiel {name} (cela peut prendre quelques minutes)", name = loader.name())));
            let mut last = String::new();
            let mut ok = false;
            for flag in ["--installClient", "--install-client"] {
                let (status, output) = run_installer(java, &installer, flag, &layout.root, cancel, sink)?;
                last = output;
                if status {
                    ok = true;
                    break;
                }
                if !last.contains("not a recognized option") && !last.contains("UnrecognizedOptionException") {
                    break;
                }
            }
            if !ok {
                let tail: Vec<&str> = last.lines().rev().take(8).collect::<Vec<_>>().into_iter().rev().collect();
                return Err(Error::Invalid(tr!("l'installateur {name} a échoué :\n{v}", name = loader.name(), v = tail.join("\n"))));
            }
            let expected = version_id(loader, mc, lv);
            if layout.version_json(&expected).is_file() {
                return Ok(expected);
            }
            let created: Vec<String> = crate::install::installed(layout).into_iter().filter(|v| !before.contains(v)).collect();
            created
                .into_iter()
                .find(|id| read_json(&layout.version_json(id)).ok().and_then(|v| v.get("inheritsFrom").and_then(|p| p.as_str()).map(|p| p == mc)).unwrap_or(false))
                .ok_or_else(|| Error::Invalid(tr!("installateur {name} terminé, mais aucune version {expected} créée", name = loader.name(), expected = expected)))
        }
    }
}

fn run_installer(java: &Path, jar: &Path, flag: &str, root: &Path, cancel: &AtomicBool, sink: Sink) -> Result<(bool, String)> {
    use std::io::BufRead;
    let mut cmd = std::process::Command::new(java);
    cmd.arg("-jar").arg(jar).arg(flag).arg(root).current_dir(root).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let mut child = cmd.spawn().map_err(|e| Error::Invalid(tr!("Java introuvable ({java}) : {e}", java = java.display(), e = e)))?;
    let stderr = child.stderr.take();
    let err_thread = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(e) = stderr {
            for l in std::io::BufReader::new(e).lines().map_while(|l| l.ok()) {
                s.push_str(&l);
                s.push('\n');
            }
        }
        s
    });
    let mut output = String::new();
    if let Some(out) = child.stdout.take() {
        for line in std::io::BufReader::new(out).lines().map_while(|l| l.ok()) {
            if cancel.load(Ordering::Relaxed) {
                let _ = child.kill();
                return Err(Error::Cancelled);
            }
            if !line.trim().is_empty() {
                sink(Event::Log(line.clone()));
            }
            output.push_str(&line);
            output.push('\n');
        }
    }
    let status = child.wait().map_err(|e| Error::Invalid(e.to_string()))?;
    output.push_str(&err_thread.join().unwrap_or_default());
    Ok((status.success(), output))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maven_and_neoforge_versions() {
        let xml = "<metadata><versioning><versions><version>1.20.1-47.2.0</version><version>1.20.1-47.3.0</version><version>1.19.2-43.2.0</version></versions></versioning></metadata>";
        assert_eq!(maven_versions(xml), vec!["1.20.1-47.2.0", "1.20.1-47.3.0", "1.19.2-43.2.0"]);
        assert_eq!(neoforge_mc("21.1.77").as_deref(), Some("1.21.1"));
        assert_eq!(neoforge_mc("20.4.237").as_deref(), Some("1.20.4"));
        assert_eq!(neoforge_mc("21.0.10-beta").as_deref(), Some("1.21"));
        assert_eq!(neoforge_mc("26.1.0.5").as_deref(), Some("26.1"));
        assert_eq!(neoforge_mc("26.1.1.2").as_deref(), Some("26.1.1"));
        assert_eq!(neoforge_mc("x"), None);
        assert_eq!(version_id(Loader::Fabric, "1.20.1", "0.15.7"), "fabric-loader-0.15.7-1.20.1");
        assert_eq!(version_id(Loader::Forge, "1.20.1", "47.2.0"), "1.20.1-forge-47.2.0");
        assert_eq!(Loader::from_id("fabric-loader"), Some(Loader::Fabric));
        assert_eq!(Loader::from_id("NeoForge"), Some(Loader::NeoForge));
    }
}

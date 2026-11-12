//! Métadonnées officielles de Mojang : liste des versions, fichiers JSON de version
//! (bibliothèques, règles par système, arguments, natives, assets, Java requis) et héritage
//! `inheritsFrom` utilisé par les mod loaders.

use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

pub const VERSION_MANIFEST: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
pub const RESOURCES: &str = "https://resources.download.minecraft.net";
pub const LIBRARIES: &str = "https://libraries.minecraft.net";

// --------------------------------------------------------------------------- liste des versions

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct ManifestEntry {
    pub id: String,
    /// `release`, `snapshot`, `old_beta`, `old_alpha`.
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
    #[serde(rename = "releaseTime")]
    pub release_time: String,
    #[serde(default)]
    pub sha1: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Manifest {
    pub latest: Latest,
    pub versions: Vec<ManifestEntry>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Latest {
    pub release: String,
    pub snapshot: String,
}

impl Manifest {
    pub fn get(&self, id: &str) -> Option<&ManifestEntry> {
        self.versions.iter().find(|v| v.id == id)
    }
}

// ------------------------------------------------------------------------------ règles

/// Contexte d'évaluation des règles d'un fichier de version.
#[derive(Clone, Debug)]
pub struct Env {
    /// `windows`, `linux`, `osx`.
    pub os: &'static str,
    /// `x86`, `x86_64`, `arm64`.
    pub arch: &'static str,
    /// Fonctionnalités actives (`has_custom_resolution`…).
    pub features: Vec<&'static str>,
}

impl Env {
    pub fn current() -> Self {
        let os = if cfg!(windows) {
            "windows"
        } else if cfg!(target_os = "macos") {
            "osx"
        } else {
            "linux"
        };
        let arch = match std::env::consts::ARCH {
            "x86" => "x86",
            "aarch64" => "arm64",
            _ => "x86_64",
        };
        Self { os, arch, features: Vec::new() }
    }
}

#[derive(Clone, Debug, Deserialize, Default)]
pub struct Rule {
    pub action: String,
    #[serde(default)]
    pub os: Option<OsRule>,
    #[serde(default)]
    pub features: Option<BTreeMap<String, bool>>,
}

#[derive(Clone, Debug, Deserialize, Default)]
pub struct OsRule {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub arch: Option<String>,
}

/// Règles de Mojang : sans règle, autorisé ; sinon la dernière règle applicable décide.
pub fn allowed(rules: &[Rule], env: &Env) -> bool {
    if rules.is_empty() {
        return true;
    }
    let mut allow = false;
    for r in rules {
        let os_ok = r.os.as_ref().is_none_or(|os| {
            os.name.as_deref().is_none_or(|n| n == env.os) && os.arch.as_deref().is_none_or(|a| a == env.arch || (a == "x86" && env.arch == "x86"))
        });
        let feat_ok = r.features.as_ref().is_none_or(|f| f.iter().all(|(k, v)| env.features.contains(&k.as_str()) == *v));
        if os_ok && feat_ok {
            allow = r.action == "allow";
        }
    }
    allow
}

// --------------------------------------------------------------------- fichier de version

#[derive(Clone, Debug, Deserialize, Default, PartialEq)]
pub struct Download {
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub url: String,
}

#[derive(Clone, Debug, Deserialize, Default)]
pub struct LibraryDownloads {
    #[serde(default)]
    pub artifact: Option<Download>,
    #[serde(default)]
    pub classifiers: BTreeMap<String, Download>,
}

#[derive(Clone, Debug, Deserialize, Default)]
pub struct Library {
    pub name: String,
    #[serde(default)]
    pub downloads: Option<LibraryDownloads>,
    /// Dépôt Maven (Fabric, Quilt, anciennes versions de Forge).
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub rules: Vec<Rule>,
    /// Natives à l'ancienne : système → classificateur (`natives-windows-${arch}`).
    #[serde(default)]
    pub natives: BTreeMap<String, String>,
    #[serde(default)]
    pub extract: Option<Extract>,
}

#[derive(Clone, Debug, Deserialize, Default)]
pub struct Extract {
    #[serde(default)]
    pub exclude: Vec<String>,
}

/// Coordonnées Maven `groupe:artefact:version[:classificateur][@extension]`.
#[derive(Clone, Debug, PartialEq)]
pub struct Coord {
    pub group: String,
    pub artifact: String,
    pub version: String,
    pub classifier: Option<String>,
    pub ext: String,
}

impl Coord {
    pub fn parse(name: &str) -> Option<Self> {
        let (main, ext) = match name.split_once('@') {
            Some((m, e)) => (m, e.to_string()),
            None => (name, "jar".to_string()),
        };
        let mut it = main.split(':');
        let group = it.next()?.to_string();
        let artifact = it.next()?.to_string();
        let version = it.next()?.to_string();
        let classifier = it.next().map(str::to_string);
        if group.is_empty() || artifact.is_empty() || version.is_empty() {
            return None;
        }
        Some(Self { group, artifact, version, classifier, ext })
    }

    /// Chemin relatif dans un dépôt Maven.
    pub fn path(&self) -> String {
        let file = match &self.classifier {
            Some(c) => format!("{}-{}-{c}.{}", self.artifact, self.version, self.ext),
            None => format!("{}-{}.{}", self.artifact, self.version, self.ext),
        };
        format!("{}/{}/{}/{file}", self.group.replace('.', "/"), self.artifact, self.version)
    }

    /// Clé de dédoublonnage (même bibliothèque, versions différentes).
    pub fn key(&self) -> String {
        format!("{}:{}:{}", self.group, self.artifact, self.classifier.as_deref().unwrap_or(""))
    }
}

/// Fichier à placer dans `libraries/`.
#[derive(Clone, Debug, PartialEq)]
pub struct LibFile {
    pub path: String,
    pub url: String,
    pub sha1: Option<String>,
    pub size: Option<u64>,
    /// Natives à extraire (ancienne forme) ou bibliothèque de natives (nouvelle forme).
    pub natives: bool,
    pub exclude: Vec<String>,
    /// Au classpath (les natives à l'ancienne n'y vont pas).
    pub classpath: bool,
    pub key: String,
}

impl Library {
    /// Fichiers nécessaires sur ce système (bibliothèque et, le cas échéant, natives).
    pub fn files(&self, env: &Env) -> Vec<LibFile> {
        if !allowed(&self.rules, env) {
            return Vec::new();
        }
        let Some(coord) = Coord::parse(&self.name) else { return Vec::new() };
        let mut out = Vec::new();
        let new_natives = coord.classifier.as_deref().is_some_and(|c| c.starts_with("natives-"));
        let exclude = self.extract.as_ref().map(|e| e.exclude.clone()).unwrap_or_default();
        match &self.downloads {
            Some(d) => {
                if let Some(a) = &d.artifact {
                    if !a.url.is_empty() || a.path.is_some() {
                        out.push(LibFile {
                            path: a.path.clone().unwrap_or_else(|| coord.path()),
                            url: a.url.clone(),
                            sha1: a.sha1.clone(),
                            size: a.size,
                            natives: new_natives,
                            exclude: exclude.clone(),
                            classpath: true,
                            key: coord.key(),
                        });
                    }
                }
            }
            None => {
                let base = self.url.clone().unwrap_or_else(|| LIBRARIES.to_string());
                let base = base.trim_end_matches('/');
                let path = coord.path();
                out.push(LibFile {
                    url: format!("{base}/{path}"),
                    path,
                    sha1: self.sha1.clone(),
                    size: None,
                    natives: new_natives,
                    exclude: exclude.clone(),
                    classpath: true,
                    key: coord.key(),
                });
            }
        }
        if let Some(classifier) = self.natives.get(env.os) {
            let classifier = classifier.replace("${arch}", if env.arch == "x86" { "32" } else { "64" });
            let from_downloads = self.downloads.as_ref().and_then(|d| d.classifiers.get(&classifier)).cloned();
            let c = Coord { classifier: Some(classifier.clone()), ..coord.clone() };
            let file = match from_downloads {
                Some(dl) => LibFile {
                    path: dl.path.clone().unwrap_or_else(|| c.path()),
                    url: dl.url.clone(),
                    sha1: dl.sha1.clone(),
                    size: dl.size,
                    natives: true,
                    exclude,
                    classpath: false,
                    key: c.key(),
                },
                None => {
                    let base = self.url.clone().unwrap_or_else(|| LIBRARIES.to_string());
                    LibFile {
                        url: format!("{}/{}", base.trim_end_matches('/'), c.path()),
                        path: c.path(),
                        sha1: None,
                        size: None,
                        natives: true,
                        exclude,
                        classpath: false,
                        key: c.key(),
                    }
                }
            };
            out.push(file);
        }
        out
    }
}

#[derive(Clone, Debug, Deserialize, Default)]
pub struct AssetIndexRef {
    pub id: String,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default, rename = "totalSize")]
    pub total_size: Option<u64>,
    pub url: String,
}

#[derive(Clone, Debug, Deserialize, Default)]
pub struct JavaVersion {
    #[serde(default)]
    pub component: String,
    #[serde(default, rename = "majorVersion")]
    pub major: u32,
}

#[derive(Clone, Debug, Deserialize, Default)]
pub struct Logging {
    #[serde(default)]
    pub client: Option<LoggingClient>,
}

#[derive(Clone, Debug, Deserialize, Default)]
pub struct LoggingClient {
    pub argument: String,
    pub file: LoggingFile,
}

#[derive(Clone, Debug, Deserialize, Default)]
pub struct LoggingFile {
    pub id: String,
    #[serde(default)]
    pub sha1: Option<String>,
    pub url: String,
}

/// Argument du nouveau format : texte, ou valeur(s) soumises à des règles.
#[derive(Clone, Debug, PartialEq)]
pub enum Arg {
    Plain(String),
    Ruled(Vec<Rule>, Vec<String>),
}

impl PartialEq for Rule {
    fn eq(&self, o: &Self) -> bool {
        self.action == o.action
    }
}

fn parse_args(v: &Value) -> Vec<Arg> {
    let Some(list) = v.as_array() else { return Vec::new() };
    list.iter()
        .filter_map(|a| {
            if let Some(s) = a.as_str() {
                return Some(Arg::Plain(s.to_string()));
            }
            let rules: Vec<Rule> = serde_json::from_value(a.get("rules")?.clone()).unwrap_or_default();
            let value = a.get("value")?;
            let values = match value {
                Value::String(s) => vec![s.clone()],
                Value::Array(v) => v.iter().filter_map(|x| x.as_str().map(str::to_string)).collect(),
                _ => return None,
            };
            Some(Arg::Ruled(rules, values))
        })
        .collect()
}

/// Fichier de version (éventuellement fusionné avec son parent).
#[derive(Clone, Debug, Default)]
pub struct Version {
    pub id: String,
    pub kind: String,
    pub inherits_from: Option<String>,
    pub main_class: String,
    pub libraries: Vec<Library>,
    pub client: Option<Download>,
    /// Version dont le `.jar` sert de client (le parent pour un loader).
    pub jar: Option<String>,
    pub asset_index: Option<AssetIndexRef>,
    pub assets: Option<String>,
    pub game_args: Vec<Arg>,
    pub jvm_args: Vec<Arg>,
    /// Ancien format (avant 1.13) : une seule chaîne.
    pub legacy_args: Option<String>,
    pub java: Option<JavaVersion>,
    pub logging: Option<LoggingClient>,
    pub release_time: Option<String>,
}

impl Version {
    pub fn parse(v: &Value) -> Result<Self, String> {
        let id = v.get("id").and_then(|x| x.as_str()).ok_or("fichier de version sans identifiant")?.to_string();
        let libraries: Vec<Library> = match v.get("libraries") {
            Some(l) => serde_json::from_value(l.clone()).map_err(|e| format!("{id} : bibliothèques illisibles ({e})"))?,
            None => Vec::new(),
        };
        let downloads = v.get("downloads");
        Ok(Self {
            kind: v.get("type").and_then(|x| x.as_str()).unwrap_or("release").to_string(),
            inherits_from: v.get("inheritsFrom").and_then(|x| x.as_str()).map(str::to_string),
            main_class: v.get("mainClass").and_then(|x| x.as_str()).unwrap_or_default().to_string(),
            libraries,
            client: downloads.and_then(|d| d.get("client")).and_then(|c| serde_json::from_value(c.clone()).ok()),
            jar: v.get("jar").and_then(|x| x.as_str()).map(str::to_string),
            asset_index: v.get("assetIndex").and_then(|a| serde_json::from_value(a.clone()).ok()),
            assets: v.get("assets").and_then(|x| x.as_str()).map(str::to_string),
            game_args: v.get("arguments").and_then(|a| a.get("game")).map(parse_args).unwrap_or_default(),
            jvm_args: v.get("arguments").and_then(|a| a.get("jvm")).map(parse_args).unwrap_or_default(),
            legacy_args: v.get("minecraftArguments").and_then(|x| x.as_str()).map(str::to_string),
            java: v.get("javaVersion").and_then(|j| serde_json::from_value(j.clone()).ok()),
            logging: v.get("logging").and_then(|l| serde_json::from_value::<Logging>(l.clone()).ok()).and_then(|l| l.client),
            release_time: v.get("releaseTime").and_then(|x| x.as_str()).map(str::to_string),
            id,
        })
    }

    /// Fusionne un fichier enfant (loader) avec son parent : les bibliothèques de l'enfant
    /// passent en premier et l'emportent, les arguments s'ajoutent, le reste vient du parent
    /// quand l'enfant ne le précise pas.
    pub fn merge(child: Version, parent: Version) -> Version {
        let mut libraries = child.libraries.clone();
        let keys: Vec<String> = child.libraries.iter().filter_map(|l| Coord::parse(&l.name)).map(|c| c.key()).collect();
        for l in parent.libraries {
            if !Coord::parse(&l.name).is_some_and(|c| keys.contains(&c.key())) {
                libraries.push(l);
            }
        }
        let legacy = child.legacy_args.clone().or(parent.legacy_args.clone());
        let mut game_args = parent.game_args.clone();
        game_args.extend(child.game_args.clone());
        let mut jvm_args = parent.jvm_args.clone();
        jvm_args.extend(child.jvm_args.clone());
        Version {
            id: child.id,
            kind: child.kind,
            inherits_from: None,
            main_class: if child.main_class.is_empty() { parent.main_class } else { child.main_class },
            libraries,
            client: child.client.or(parent.client),
            jar: child.jar.or(Some(parent.jar.unwrap_or(parent.id))),
            asset_index: child.asset_index.or(parent.asset_index),
            assets: child.assets.or(parent.assets),
            // Ancien format : l'enfant redonne la liste complète.
            game_args: if child.legacy_args.is_some() { Vec::new() } else { game_args },
            jvm_args,
            legacy_args: legacy,
            java: child.java.or(parent.java),
            logging: child.logging.or(parent.logging),
            release_time: child.release_time.or(parent.release_time),
        }
    }

    /// Bibliothèques à télécharger sur ce système, sans doublon.
    pub fn library_files(&self, env: &Env) -> Vec<LibFile> {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for lib in &self.libraries {
            for f in lib.files(env) {
                if seen.insert((f.key.clone(), f.natives && !f.classpath)) {
                    out.push(f);
                }
            }
        }
        out
    }

    /// Version de Java demandée (8 pour les anciennes versions qui ne la précisent pas).
    pub fn java_major(&self) -> u32 {
        self.java.as_ref().map(|j| j.major).filter(|m| *m > 0).unwrap_or(8)
    }

    pub fn java_component(&self) -> String {
        self.java.as_ref().map(|j| j.component.clone()).filter(|c| !c.is_empty()).unwrap_or_else(|| "jre-legacy".into())
    }
}

/// Index des assets : chemin logique → (empreinte, taille).
#[derive(Clone, Debug, Default)]
pub struct AssetIndex {
    pub objects: Vec<(String, String, u64)>,
    /// Versions anciennes : copie sous `assets/virtual/<id>`.
    pub virtual_: bool,
    /// Très anciennes versions : copie dans `<jeu>/resources`.
    pub map_to_resources: bool,
}

impl AssetIndex {
    pub fn parse(v: &Value) -> Self {
        let mut objects: Vec<(String, String, u64)> = v
            .get("objects")
            .and_then(|o| o.as_object())
            .map(|o| {
                o.iter()
                    .filter_map(|(k, x)| Some((k.clone(), x.get("hash")?.as_str()?.to_string(), x.get("size").and_then(|s| s.as_u64()).unwrap_or(0))))
                    .filter(|(_, h, _)| h.len() == 40 && h.chars().all(|c| c.is_ascii_hexdigit()))
                    .collect()
            })
            .unwrap_or_default();
        objects.sort();
        Self {
            objects,
            virtual_: v.get("virtual").and_then(|x| x.as_bool()).unwrap_or(false),
            map_to_resources: v.get("map_to_resources").and_then(|x| x.as_bool()).unwrap_or(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn win() -> Env {
        Env { os: "windows", arch: "x86_64", features: vec![] }
    }

    #[test]
    fn rules_follow_mojang_semantics() {
        let r: Vec<Rule> = serde_json::from_value(json!([{"action":"allow"},{"action":"disallow","os":{"name":"osx"}}])).unwrap();
        assert!(allowed(&r, &win()));
        assert!(!allowed(&r, &Env { os: "osx", ..win() }));
        let r: Vec<Rule> = serde_json::from_value(json!([{"action":"allow","os":{"name":"linux"}}])).unwrap();
        assert!(!allowed(&r, &win()));
        let r: Vec<Rule> = serde_json::from_value(json!([{"action":"allow","features":{"has_custom_resolution":true}}])).unwrap();
        assert!(!allowed(&r, &win()));
        assert!(allowed(&r, &Env { features: vec!["has_custom_resolution"], ..win() }));
        assert!(allowed(&[], &win()));
    }

    #[test]
    fn maven_coordinates() {
        let c = Coord::parse("org.lwjgl:lwjgl:3.3.1:natives-windows").unwrap();
        assert_eq!(c.path(), "org/lwjgl/lwjgl/3.3.1/lwjgl-3.3.1-natives-windows.jar");
        assert_eq!(Coord::parse("net.fabricmc:intermediary:1.20.1").unwrap().path(), "net/fabricmc/intermediary/1.20.1/intermediary-1.20.1.jar");
        assert_eq!(Coord::parse("de.oceanlabs.mcp:mcp_config:1.20.1@zip").unwrap().path(), "de/oceanlabs/mcp/mcp_config/1.20.1/mcp_config-1.20.1.zip");
        assert!(Coord::parse("pas-une-coordonnee").is_none());
    }

    #[test]
    fn libraries_native_old_and_new_style() {
        // Forme réelle des fichiers de Mojang (1.12.2 et 1.20.1), réduite.
        let v = Version::parse(&json!({
            "id": "test", "mainClass": "net.minecraft.client.main.Main",
            "libraries": [
                {"name": "org.lwjgl.lwjgl:lwjgl-platform:2.9.4-nightly-20150209",
                 "downloads": {"artifact": {"path":"a.jar","sha1":"1","size":1,"url":"https://x/a.jar"},
                    "classifiers": {"natives-windows": {"path":"n.jar","sha1":"2","size":2,"url":"https://x/n.jar"},
                                    "natives-linux": {"path":"l.jar","sha1":"3","size":3,"url":"https://x/l.jar"}}},
                 "natives": {"windows":"natives-windows","linux":"natives-linux"}, "extract": {"exclude":["META-INF/"]}},
                {"name": "org.lwjgl:lwjgl:3.3.1:natives-windows",
                 "downloads": {"artifact": {"path":"org/lwjgl/lwjgl/3.3.1/lwjgl-3.3.1-natives-windows.jar","sha1":"4","size":4,"url":"https://x/w.jar"}},
                 "rules": [{"action":"allow","os":{"name":"windows"}}]},
                {"name": "org.lwjgl:lwjgl:3.3.1:natives-linux",
                 "downloads": {"artifact": {"path":"l2.jar","sha1":"5","size":5,"url":"https://x/l2.jar"}},
                 "rules": [{"action":"allow","os":{"name":"linux"}}]},
                {"name": "net.fabricmc:fabric-loader:0.15.7", "url": "https://maven.fabricmc.net/"}
            ]
        }))
        .unwrap();
        let files = v.library_files(&win());
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, vec!["a.jar", "n.jar", "org/lwjgl/lwjgl/3.3.1/lwjgl-3.3.1-natives-windows.jar", "net/fabricmc/fabric-loader/0.15.7/fabric-loader-0.15.7.jar"]);
        assert!(files[1].natives && !files[1].classpath && files[1].exclude == vec!["META-INF/".to_string()]);
        assert!(files[2].natives && files[2].classpath);
        assert_eq!(files[3].url, "https://maven.fabricmc.net/net/fabricmc/fabric-loader/0.15.7/fabric-loader-0.15.7.jar");
    }

    #[test]
    fn loader_profile_merges_with_parent() {
        let parent = Version::parse(&json!({
            "id": "1.20.1", "mainClass": "net.minecraft.client.main.Main", "type": "release",
            "assetIndex": {"id":"5","url":"https://x/5.json"}, "assets": "5",
            "javaVersion": {"component":"java-runtime-gamma","majorVersion":17},
            "downloads": {"client": {"sha1":"c","size":1,"url":"https://x/client.jar"}},
            "arguments": {"game": ["--username","${auth_player_name}"], "jvm": ["-cp","${classpath}"]},
            "libraries": [{"name":"org.ow2.asm:asm:9.3","downloads":{"artifact":{"path":"asm93.jar","url":"https://x/asm93.jar"}}},
                          {"name":"com.mojang:brigadier:1.1.8","downloads":{"artifact":{"path":"b.jar","url":"https://x/b.jar"}}}]
        }))
        .unwrap();
        let child = Version::parse(&json!({
            "id": "fabric-loader-0.15.7-1.20.1", "inheritsFrom": "1.20.1", "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient",
            "arguments": {"game": [], "jvm": ["-DFabricMcEmu= net.minecraft.client.main.Main "]},
            "libraries": [{"name":"org.ow2.asm:asm:9.6","url":"https://maven.fabricmc.net/"}]
        }))
        .unwrap();
        assert_eq!(child.inherits_from.as_deref(), Some("1.20.1"));
        let m = Version::merge(child, parent);
        assert_eq!(m.main_class, "net.fabricmc.loader.impl.launch.knot.KnotClient");
        assert_eq!(m.jar.as_deref(), Some("1.20.1"));
        assert_eq!(m.java_major(), 17);
        let names: Vec<&str> = m.libraries.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, vec!["org.ow2.asm:asm:9.6", "com.mojang:brigadier:1.1.8"], "asm du loader prioritaire, sans doublon");
        assert_eq!(m.jvm_args.len(), 3);
        assert_eq!(m.asset_index.unwrap().id, "5");
    }

    #[test]
    fn asset_index_and_manifest() {
        let a = AssetIndex::parse(&json!({"objects": {"minecraft/sounds/a.ogg": {"hash":"0123456789abcdef0123456789abcdef01234567","size":10}, "bad": {"hash":"zz","size":1}}, "virtual": true}));
        assert_eq!(a.objects.len(), 1);
        assert!(a.virtual_);
        let m: Manifest = serde_json::from_value(json!({"latest":{"release":"1.21.4","snapshot":"25w02a"},
            "versions":[{"id":"1.21.4","type":"release","url":"https://piston-meta.mojang.com/v1/packages/x/1.21.4.json","time":"t","releaseTime":"2024-12-03T10:12:57+00:00","sha1":"x","complianceLevel":1}]})).unwrap();
        assert_eq!(m.get("1.21.4").unwrap().kind, "release");
    }
}

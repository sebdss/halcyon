//! Emplacements de sauvegarde connus, d'après la base communautaire **Ludusavi**
//! (<https://github.com/mtkennerly/ludusavi-manifest>, licence MIT) : pour des dizaines de
//! milliers de jeux, les fichiers de sauvegarde exacts, par système et par boutique.
//!
//! - Le manifeste (YAML, ~18 Mo) est téléchargé au plus une fois par semaine, et seulement
//!   s'il a changé (`ETag`). Hors ligne, la dernière copie sert.
//! - Il est lu par un analyseur **strict** du sous-ensemble YAML qu'il utilise (blocs indentés,
//!   clés et valeurs simples ou entre guillemets, `{}`). Toute autre syntaxe le fait refuser :
//!   une base mal lue ne propose jamais de dossier au hasard.
//! - Un jeu est retrouvé par son numéro Steam, puis GOG, puis par son nom ou son dossier.
//! - Les chemins (`<winAppData>/Jeu/*.sav`) sont résolus sur ce PC ; sous Linux, les règles
//!   Windows d'un jeu Steam sont aussi cherchées dans son préfixe Proton. Seul le dossier fixe
//!   qui précède le premier joker est proposé, et jamais une racine entière (dossier du jeu,
//!   Documents…) : Halcyon sauvegarde des dossiers, pas des motifs.

use halcyon_core::{util, GameSource};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub const MANIFEST_URL: &str = "https://raw.githubusercontent.com/mtkennerly/ludusavi-manifest/master/data/manifest.yaml";
/// Plafond de téléchargement (le manifeste fait ~18 Mo et grossit lentement).
pub const MAX_MANIFEST: u64 = 96 * 1024 * 1024;
/// Délai entre deux vérifications du manifeste.
pub const REFRESH_SECS: i64 = 7 * 86_400;

// ------------------------------------------------------------------ analyseur YAML (sous-ensemble)

struct Line<'a> {
    no: usize,
    indent: usize,
    text: &'a str,
}

fn err(l: &Line, what: &str) -> String {
    tr!("base Ludusavi, ligne {no} : {what}", no = l.no, what = what)
}

/// Lit un scalaire : entre guillemets (échappements JSON), sinon texte, nombre ou booléen.
fn scalar(raw: &str, l: &Line) -> Result<Value, String> {
    let raw = raw.trim();
    if raw.starts_with('"') {
        return serde_json::from_str::<String>(raw).map(Value::String).map_err(|_| err(l, &tr!("texte entre guillemets invalide")));
    }
    if raw.starts_with(['\'', '[', '{', '|', '>', '&', '*', '!', '%', '@', '`']) {
        return match raw {
            "{}" => Ok(Value::Object(Map::new())),
            "[]" => Ok(Value::Array(Vec::new())),
            _ => Err(err(l, &tr!("syntaxe YAML non prise en charge"))),
        };
    }
    Ok(match raw {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        "null" | "~" | "" => Value::Null,
        _ => raw.parse::<i64>().map(Value::from).unwrap_or_else(|_| Value::String(raw.to_string())),
    })
}

/// Sépare `clé: reste` (clé entre guillemets ou simple).
fn key_value<'a>(text: &'a str, l: &Line) -> Result<(String, &'a str), String> {
    if text.starts_with('"') {
        // Fin de la chaîne : premier guillemet non échappé.
        let bytes = text.as_bytes();
        let mut i = 1;
        while i < bytes.len() {
            match bytes[i] {
                b'\\' => i += 2,
                b'"' => break,
                _ => i += 1,
            }
        }
        if i >= bytes.len() {
            return Err(err(l, &tr!("clé entre guillemets non terminée")));
        }
        let key: String = serde_json::from_str(&text[..=i]).map_err(|_| err(l, &tr!("clé entre guillemets invalide")))?;
        let rest = text[i + 1..].strip_prefix(':').ok_or_else(|| err(l, &tr!("« : » attendu après la clé")))?;
        if !(rest.is_empty() || rest.starts_with(' ')) {
            return Err(err(l, &tr!("« : » attendu après la clé")));
        }
        return Ok((key, rest.trim_start()));
    }
    if let Some(at) = text.find(": ") {
        return Ok((text[..at].to_string(), text[at + 2..].trim_start()));
    }
    text.strip_suffix(':').map(|k| (k.to_string(), "")).ok_or_else(|| err(l, &tr!("« clé: valeur » attendu")))
}

/// Bloc à l'indentation `indent` à partir de `lines[*i]` : table ou liste.
fn block(lines: &[Line], i: &mut usize, indent: usize) -> Result<Value, String> {
    let first = &lines[*i];
    if first.text == "-" || first.text.starts_with("- ") {
        let mut items = Vec::new();
        while *i < lines.len() && lines[*i].indent == indent && (lines[*i].text == "-" || lines[*i].text.starts_with("- ")) {
            let l = &lines[*i];
            let rest = l.text[1..].trim_start();
            let looks_like_map = !rest.starts_with('"') && (rest.contains(": ") || rest.ends_with(':'))
                || rest.starts_with('"') && key_value(rest, l).is_ok();
            if looks_like_map {
                // « - clé: valeur » : table dont les autres clés sont alignées sur la première.
                items.push(map_from(lines, i, indent + 2, Some(rest))?);
            } else {
                items.push(scalar(rest, l)?);
                *i += 1;
            }
        }
        return Ok(Value::Array(items));
    }
    map_from(lines, i, indent, None)
}

fn map_from(lines: &[Line], i: &mut usize, indent: usize, inline_first: Option<&str>) -> Result<Value, String> {
    let mut map = Map::new();
    let mut inline = inline_first;
    loop {
        let (l, text) = match inline.take() {
            Some(t) => (&lines[*i], t),
            None => {
                if *i >= lines.len() || lines[*i].indent != indent || lines[*i].text.starts_with("- ") {
                    break;
                }
                (&lines[*i], lines[*i].text)
            }
        };
        let (key, rest) = key_value(text, l)?;
        *i += 1;
        let value = if rest.is_empty() {
            match lines.get(*i) {
                Some(next) if next.indent > indent || next.indent == indent && next.text.starts_with("- ") => block(lines, i, next.indent)?,
                _ => Value::Null,
            }
        } else {
            scalar(rest, l)?
        };
        map.insert(key, value);
    }
    if *i < lines.len() && lines[*i].indent > indent {
        return Err(err(&lines[*i], &tr!("indentation inattendue")));
    }
    Ok(Value::Object(map))
}

/// Analyse un document YAML du sous-ensemble pris en charge.
pub fn parse_yaml(text: &str) -> Result<Value, String> {
    let mut lines = Vec::new();
    for (n, raw) in text.lines().enumerate() {
        let trimmed = raw.trim_end();
        let content = trimmed.trim_start_matches(' ');
        if content.is_empty() || content.starts_with('#') || content == "---" {
            continue;
        }
        let l = Line { no: n + 1, indent: trimmed.len() - content.len(), text: content };
        if content.starts_with('\t') {
            return Err(err(&l, &tr!("tabulation")));
        }
        lines.push(l);
    }
    if lines.is_empty() {
        return Ok(Value::Object(Map::new()));
    }
    let mut i = 0;
    let v = block(&lines, &mut i, lines[0].indent)?;
    if i < lines.len() {
        return Err(err(&lines[i], &tr!("contenu inattendu")));
    }
    Ok(v)
}

// ------------------------------------------------------------------ index

/// Règle de fichiers : motif et conditions (`when`).
#[derive(Clone, Debug, PartialEq)]
pub struct FileRule {
    pub pattern: String,
    /// (système, boutique) ; vide : toujours.
    pub when: Vec<(Option<String>, Option<String>)>,
}

#[derive(Clone, Debug, Default)]
pub struct Entry {
    pub name: String,
    pub files: Vec<FileRule>,
}

/// Index des jeux ayant des fichiers de sauvegarde connus.
#[derive(Default)]
pub struct Index {
    entries: Vec<Entry>,
    by_steam: HashMap<u64, usize>,
    by_gog: HashMap<u64, usize>,
    by_name: HashMap<String, usize>,
    by_folder: HashMap<String, usize>,
    /// Premiers dossiers (`<winAppData>/mmfapplications`) cités par plusieurs jeux : partagés.
    shared: HashSet<String>,
}

/// `<repère>/premier dossier`, en minuscules : sert à repérer les dossiers partagés.
fn first_folder(pattern: &str) -> Option<String> {
    let (head, tail) = pattern.split_once('>')?;
    let first = tail.split('/').find(|p| !p.is_empty())?;
    (!first.contains(['*', '?', '[', '<', '{'])).then(|| format!("{head}>/{}", first.to_lowercase()))
}

fn key(s: &str) -> String {
    util::search_key(s).replace(' ', "")
}

/// Numéro principal (`steam.id`) et numéros secondaires (`id.steamExtra`).
fn ids(v: &Value, store: &str, extra: &str) -> Vec<u64> {
    let mut out: Vec<u64> = v.pointer(&format!("/{store}/id")).and_then(Value::as_u64).into_iter().collect();
    if let Some(a) = v.pointer(&format!("/id/{extra}")).and_then(Value::as_array) {
        out.extend(a.iter().filter_map(Value::as_u64));
    }
    out
}

impl Index {
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Construit l'index à partir du manifeste (texte YAML). Les jeux sont lus un par un :
    /// la mémoire utilisée reste celle de l'index, pas celle du document entier.
    pub fn from_manifest(text: &str) -> Result<Self, String> {
        let mut idx = Index::default();
        let mut owners: HashMap<String, usize> = HashMap::new();
        let mut aliases: Vec<(String, String)> = Vec::new();
        let mut start: Option<usize> = None;
        let mut offset = 0;
        let mut chunks: Vec<(usize, usize)> = Vec::new();
        for line in text.split_inclusive('\n') {
            let top = !line.starts_with([' ', '\n', '\r', '#']) && !line.trim().is_empty() && line.trim_end() != "---";
            if top {
                if let Some(s) = start {
                    chunks.push((s, offset));
                }
                start = Some(offset);
            }
            offset += line.len();
        }
        if let Some(s) = start {
            chunks.push((s, text.len()));
        }
        if chunks.len() < 100 {
            return Err(tr!("base Ludusavi incomplète ({n} jeux)", n = chunks.len()));
        }
        for (a, b) in chunks {
            let doc = parse_yaml(&text[a..b])?;
            let Some((name, v)) = doc.as_object().and_then(|m| m.iter().next()) else { continue };
            if let Some(target) = v.get("alias").and_then(Value::as_str) {
                aliases.push((name.clone(), target.to_string()));
                continue;
            }
            let files: Vec<FileRule> = v
                .get("files")
                .and_then(Value::as_object)
                .map(|m| {
                    m.iter()
                        .filter(|(_, rule)| {
                            // Sauvegardes (« save ») ou fichiers sans étiquette ; la configuration seule est ignorée.
                            rule.get("tags").and_then(Value::as_array).is_none_or(|t| t.iter().any(|x| x.as_str() == Some("save")))
                        })
                        .map(|(pattern, rule)| FileRule {
                            pattern: pattern.clone(),
                            when: rule
                                .get("when")
                                .and_then(Value::as_array)
                                .map(|w| {
                                    w.iter()
                                        .map(|c| (c.get("os").and_then(Value::as_str).map(str::to_string), c.get("store").and_then(Value::as_str).map(str::to_string)))
                                        .collect()
                                })
                                .unwrap_or_default(),
                        })
                        .collect()
                })
                .unwrap_or_default();
            if files.is_empty() {
                continue;
            }
            let n = idx.entries.len();
            for f in files.iter().filter_map(|r| first_folder(&r.pattern)) {
                match owners.get(&f) {
                    Some(&o) if o != n => {
                        idx.shared.insert(f);
                    }
                    _ => {
                        owners.insert(f, n);
                    }
                }
            }
            for id in ids(v, "steam", "steamExtra") {
                idx.by_steam.entry(id).or_insert(n);
            }
            for id in ids(v, "gog", "gogExtra") {
                idx.by_gog.entry(id).or_insert(n);
            }
            idx.by_name.entry(key(name)).or_insert(n);
            if let Some(dirs) = v.get("installDir").and_then(Value::as_object) {
                for d in dirs.keys() {
                    idx.by_folder.entry(key(d)).or_insert(n);
                }
            }
            idx.entries.push(Entry { name: name.clone(), files });
        }
        for (alias, target) in aliases {
            if let Some(&n) = idx.by_name.get(&key(&target)) {
                idx.by_name.entry(key(&alias)).or_insert(n);
            }
        }
        Ok(idx)
    }

    /// Entrée correspondant au jeu.
    pub fn find(&self, q: &Query) -> Option<&Entry> {
        let num = q.source_key.parse::<u64>().ok();
        let by_id = match (q.source, num) {
            (Some(GameSource::Steam), Some(id)) => self.by_steam.get(&id),
            (Some(GameSource::Gog), Some(id)) => self.by_gog.get(&id),
            _ => None,
        };
        let folder = q.install_dir.as_deref().and_then(Path::file_name).map(|f| key(&f.to_string_lossy()));
        by_id
            .or_else(|| self.by_name.get(&key(&q.name)))
            .or_else(|| folder.filter(|f| f.len() >= 3).and_then(|f| self.by_folder.get(&f)))
            .and_then(|&n| self.entries.get(n))
    }

    /// Dossiers (ou fichiers) de sauvegarde du jeu sur ce PC. L'existence n'est pas vérifiée.
    pub fn locations(&self, q: &Query, env: &Places) -> Vec<PathBuf> {
        let Some(entry) = self.find(q) else { return Vec::new() };
        let os = if cfg!(windows) {
            "windows"
        } else if cfg!(target_os = "macos") {
            "mac"
        } else {
            "linux"
        };
        let store = q.source.map(store_name).unwrap_or("other");
        let mut out = Vec::new();
        for rule in &entry.files {
            for proton in [false, true] {
                // Préfixe Proton : règles Windows d'un jeu Steam, sous Linux.
                if proton && env.proton.is_none() {
                    continue;
                }
                let want_os = if proton { "windows" } else { os };
                let applies = rule.when.is_empty()
                    || rule.when.iter().any(|(o, s)| o.as_deref().is_none_or(|o| o == want_os) && s.as_deref().is_none_or(|s| s == store));
                // Sans condition de système, une règle à chemins Windows ne vaut que là où ils existent.
                if !applies || (proton && rule.when.iter().all(|(o, _)| o.is_none()) && !rule.pattern.contains("<win")) {
                    continue;
                }
                let shared = first_folder(&rule.pattern).is_none_or(|f| self.shared.contains(&f));
                if let Some(p) = resolve(&rule.pattern, q, env, proton, &entry.name, shared) {
                    if !out.contains(&p) {
                        out.push(p);
                    }
                }
            }
        }
        out
    }
}

/// Nom de boutique dans le manifeste.
fn store_name(s: GameSource) -> &'static str {
    match s {
        GameSource::Steam => "steam",
        GameSource::Gog => "gog",
        GameSource::Epic => "epic",
        GameSource::Ubisoft => "uplay",
        GameSource::Ea => "ea",
        GameSource::Xbox => "microsoft",
        GameSource::Itch => "itch",
        GameSource::Amazon => "prime",
        GameSource::BattleNet | GameSource::Riot => "other",
        _ => "other",
    }
}

/// Jeu recherché.
#[derive(Clone, Debug, Default)]
pub struct Query {
    pub name: String,
    pub source: Option<GameSource>,
    pub source_key: String,
    pub install_dir: Option<PathBuf>,
}

/// Dossiers de ce PC utilisés pour résoudre les chemins du manifeste.
#[derive(Clone, Debug, Default)]
pub struct Places {
    pub home: PathBuf,
    pub documents: Option<PathBuf>,
    pub app_data: Option<PathBuf>,
    pub local_app_data: Option<PathBuf>,
    pub xdg_config: Option<PathBuf>,
    pub xdg_data: Option<PathBuf>,
    pub steam_root: Option<PathBuf>,
    /// Numéro de compte Steam (dossiers `userdata/<n>`).
    pub steam_account: Option<u64>,
    /// Utilisateur Windows du préfixe Proton (`…/pfx/drive_c/users/steamuser`) du jeu.
    pub proton: Option<PathBuf>,
}

impl Places {
    pub fn current(steam_root: Option<PathBuf>, steam_account: Option<u64>) -> Self {
        Self {
            home: dirs::home_dir().unwrap_or_default(),
            documents: dirs::document_dir(),
            app_data: if cfg!(windows) { dirs::config_dir() } else { None },
            local_app_data: if cfg!(windows) { dirs::data_local_dir() } else { None },
            xdg_config: if cfg!(windows) { None } else { dirs::config_dir() },
            xdg_data: if cfg!(windows) { None } else { dirs::data_dir() },
            steam_root,
            steam_account,
            proton: None,
        }
    }

    /// Ajoute le préfixe Proton d'un jeu Steam (Linux), s'il existe.
    pub fn with_proton(mut self, appid: &str, install_dir: Option<&Path>) -> Self {
        if cfg!(windows) || appid.parse::<u64>().is_err() {
            return self;
        }
        let mut prefixes: Vec<PathBuf> = self.steam_root.iter().map(|r| r.join("steamapps/compatdata").join(appid)).collect();
        if let Some(lib) = install_dir.and_then(Path::parent).and_then(Path::parent) {
            prefixes.push(lib.join("compatdata").join(appid));
        }
        self.proton = prefixes.into_iter().map(|p| p.join("pfx/drive_c/users/steamuser")).find(|p| p.is_dir());
        self
    }
}

/// Résout un motif sur ce PC : dossier fixe qui précède le premier joker.
/// `shared` : le premier dossier du motif est cité par d'autres jeux du manifeste.
fn resolve(pattern: &str, q: &Query, env: &Places, proton: bool, game: &str, shared: bool) -> Option<PathBuf> {
    let (head, tail) = match pattern.find('>') {
        Some(at) if pattern.starts_with('<') => (&pattern[..=at], &pattern[at + 1..]),
        _ => return None, // chemin absolu sans racine connue : ignoré
    };
    let user = env.proton.as_ref().filter(|_| proton);
    let drive = user.and_then(|u| u.parent()).and_then(Path::parent);
    let base: PathBuf = match (head, user) {
        ("<base>", _) => q.install_dir.clone()?,
        ("<home>", Some(u)) => u.clone(),
        ("<home>", None) => env.home.clone(),
        ("<winAppData>", Some(u)) => u.join("AppData/Roaming"),
        ("<winLocalAppData>", Some(u)) => u.join("AppData/Local"),
        ("<winLocalAppDataLow>", Some(u)) => u.join("AppData/LocalLow"),
        ("<winDocuments>", Some(u)) => u.join("Documents"),
        ("<winPublic>", Some(_)) => drive?.join("users/Public"),
        ("<winProgramData>", Some(_)) => drive?.join("ProgramData"),
        ("<winDir>", Some(_)) => drive?.join("windows"),
        ("<winAppData>", None) => env.app_data.clone()?,
        ("<winLocalAppData>", None) => env.local_app_data.clone()?,
        ("<winLocalAppDataLow>", None) => env.local_app_data.as_ref().and_then(|d| d.parent()).map(|d| d.join("LocalLow"))?,
        ("<winDocuments>", None) => env.documents.clone().filter(|_| cfg!(windows))?,
        ("<winPublic>", None) if cfg!(windows) => PathBuf::from(std::env::var_os("PUBLIC")?),
        ("<winProgramData>", None) if cfg!(windows) => PathBuf::from(std::env::var_os("ProgramData")?),
        ("<winDir>", None) if cfg!(windows) => PathBuf::from(std::env::var_os("SystemRoot")?),
        ("<xdgConfig>", None) => env.xdg_config.clone()?,
        ("<xdgData>", None) => env.xdg_data.clone()?,
        ("<root>", _) if q.source == Some(GameSource::Steam) => env.steam_root.clone()?,
        _ => return None,
    };
    let game_folder = q.install_dir.as_deref().and_then(Path::file_name).map(|f| f.to_string_lossy().into_owned());
    let user_name = std::env::var("USERNAME").or_else(|_| std::env::var("USER")).ok();
    let mut path = base;
    let mut literal = 0;
    let mut cut = false;
    let mut previous = String::new();
    for part in tail.split('/').filter(|p| !p.is_empty()) {
        let part = match part {
            // `<root>/userdata/<compte>` : le compte Steam de ce PC (sinon : tous les comptes,
            // c'est-à-dire tout `userdata`, refusé plus bas).
            "<storeUserId>" if previous.eq_ignore_ascii_case("userdata") && q.source == Some(GameSource::Steam) => env.steam_account?.to_string(),
            "<game>" => game_folder.clone()?,
            "<osUserName>" if !proton => user_name.clone()?,
            "<osUserName>" => "steamuser".to_string(),
            p => p.to_string(),
        };
        // Premier joker ou repère variable (`<storeUserId>` : numéro de compte dont la forme
        // dépend du jeu) : on s'arrête au dossier précédent, qui contient tous les comptes.
        if part.contains(['*', '?', '[', '<', '{']) {
            cut = true;
            break;
        }
        // Le numéro de compte ne rend pas un dossier propre au jeu (`userdata/42/*`).
        if !(previous.eq_ignore_ascii_case("userdata") && part.chars().all(|c| c.is_ascii_digit())) {
            literal += 1;
        }
        previous = part.clone();
        path.push(part);
    }
    // Chemin coupé à un joker : le dossier gardé contient aussi d'autres fichiers. Il doit être
    // propre au jeu, sinon une restauration écraserait les sauvegardes d'autres jeux
    // (`AppData/MMFApplications`, `IsolatedStorage`…) ou copierait ses données (`<base>/Data`).
    // Un chemin exact du manifeste (dossier ou fichier nommé) est gardé tel quel.
    let save_like = |name: &str| {
        let n = name.to_ascii_lowercase();
        ["save", "profile", "slot", "user", "progress"].iter().any(|k| n.contains(k))
    };
    // Dossier qui porte le nom du jeu (« EldenRing » pour « Elden Ring »), pas celui d'une
    // série ou d'un studio (« ArcadeGameSeries » pour « Arcade Game Series: Dig Dug »).
    let named_after_game = |name: &str| {
        let f = key(name);
        [game, q.name.as_str()].iter().map(|g| key(g)).any(|g| {
            let (short, long) = if f.len() <= g.len() { (&f, &g) } else { (&g, &f) };
            short.len() >= 4 && long.contains(short.as_str()) && short.len() * 10 >= long.len() * 8
        })
    };
    let enough = match (cut, head) {
        (false, _) => literal >= 1,
        (true, "<base>") => literal >= 2 || literal == 1 && save_like(&previous),
        (true, _) => literal >= 2 || literal == 1 && (!shared || named_after_game(&previous)),
    };
    enough.then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"---
"! That Bastard Is Trying To Steal Our Gold !":
  installDir:
    "! That Bastard Is Trying To Steal Our Gold !": {}
  steam:
    id: 449940
"'83":
  files:
    "<winLocalAppData>/XV83/Saved/Config/Windows/*.ini":
      when:
        - os: windows
          store: steam
    "<winLocalAppData>/XV83/Saved/SaveGames/**/*.sav":
      tags:
        - save
      when:
        - os: windows
          store: steam
  id:
    steamExtra:
      - 2680660
  installDir:
    "'83": {}
  steam:
    id: 1059220
Celeste:
  files:
    "<base>/Saves":
      tags:
        - save
    "<xdgData>/Celeste/Saves":
      tags:
        - save
      when:
        - os: linux
    "<base>/*.txt":
      tags:
        - save
    "<base>/settings.ini":
      tags:
        - config
  gog:
    id: 1234
  installDir:
    Celeste Game: {}
  launch:
    "<base>/Celeste.exe":
      - arguments: "-conf \"..\\x.conf\" \u007f"
        when:
          - store: steam
Celeste Classic:
  alias: Celeste
"#;

    fn sample_index() -> Index {
        // Le manifeste réel contient des milliers de jeux : on duplique l'exemple.
        let mut text = String::from(SAMPLE);
        for i in 0..120 {
            text.push_str(&format!("Jeu {i}:\n  steam:\n    id: {}\n", 900_000 + i));
        }
        Index::from_manifest(&text).unwrap()
    }

    #[test]
    fn parses_the_manifest_subset() {
        let v = parse_yaml(SAMPLE).unwrap();
        assert_eq!(v.pointer("/'83/steam/id").and_then(Value::as_u64), Some(1_059_220));
        assert_eq!(v.pointer("/'83/id/steamExtra/0").and_then(Value::as_u64), Some(2_680_660));
        assert_eq!(v.pointer("/Celeste/launch/<base>~1Celeste.exe/0/arguments").and_then(Value::as_str), Some("-conf \"..\\x.conf\" \u{7f}"));
        assert_eq!(v.pointer("/Celeste/launch/<base>~1Celeste.exe/0/when/0/store").and_then(Value::as_str), Some("steam"));
        assert!(v.pointer("/! That Bastard Is Trying To Steal Our Gold !/installDir").is_some_and(|d| d.is_object()));
        // Syntaxe hors du sous-ensemble : refusée.
        for bad in ["a: [1, 2]", "a: |\n  x", "a:\n\t- b", "a: 'x'", "a: &x 1", "\"a: 1"] {
            assert!(parse_yaml(bad).is_err(), "{bad}");
        }
        assert!(parse_yaml("a:\n  b: 1\n    c: 2").is_err());
    }

    #[test]
    fn index_finds_games_and_keeps_save_rules_only() {
        let idx = sample_index();
        assert_eq!(idx.len(), 2, "jeux sans fichiers ignorés");
        let steam = Query { name: "Autre nom".into(), source: Some(GameSource::Steam), source_key: "2680660".into(), install_dir: None };
        let e = idx.find(&steam).unwrap();
        assert_eq!(e.name, "'83");
        // Règle sans étiquette (souvent la configuration) : gardée, comme le fait Ludusavi.
        assert_eq!(e.files.len(), 2);
        let gog = Query { name: "x".into(), source: Some(GameSource::Gog), source_key: "1234".into(), install_dir: None };
        assert_eq!(idx.find(&gog).unwrap().name, "Celeste");
        let alias = Query { name: "Celeste Classic".into(), ..Default::default() };
        assert_eq!(idx.find(&alias).unwrap().name, "Celeste");
        let folder = Query { name: "Inconnu".into(), install_dir: Some(PathBuf::from("/jeux/Celeste Game")), ..Default::default() };
        assert_eq!(idx.find(&folder).unwrap().name, "Celeste");
        // Dossier trop court (« 83 ») : pas de correspondance par dossier.
        let short = Query { name: "Inconnu".into(), install_dir: Some(PathBuf::from("/jeux/'83")), ..Default::default() };
        assert!(idx.find(&short).is_none());
        assert!(Index::from_manifest("a:\n  b: 1\n").is_err(), "base trop petite");
    }

    #[test]
    fn resolves_paths_without_whole_roots() {
        let idx = sample_index();
        let t = tempfile::tempdir().unwrap();
        let env = Places {
            home: t.path().join("home"),
            steam_account: Some(42),
            xdg_data: Some(t.path().join("data")),
            local_app_data: Some(t.path().join("local")),
            ..Default::default()
        };
        let q = Query { name: "Celeste".into(), source: Some(GameSource::Itch), source_key: "9".into(), install_dir: Some(t.path().join("Celeste")) };
        let locs = idx.locations(&q, &env);
        assert!(locs.contains(&t.path().join("Celeste/Saves")));
        // « <base>/*.txt » désignerait tout le dossier du jeu : ignoré.
        assert!(!locs.contains(&t.path().join("Celeste")));
        // Dossier partagé par plusieurs jeux, ou données du jeu : refusés après un joker.
        for (pattern, ok) in [
            ("<winAppData>/MMFApplications/LMap*.arr", false),
            ("<base>/Data/*.data", false),
            ("<base>/SaveData/*.*", true),
            ("<home>/Studio/Jeu/*.sav", true),
            ("<root>/userdata/<storeUserId>/525480/remote/savedata", true),
            ("<root>/userdata/<storeUserId>/*", false),
            ("<home>/.config/jeu.cfg", true),
            ("<winAppData>/Montagne/<storeUserId>/*.sl2", true),
            ("<winAppData>/MontagneSerie/<storeUserId>/*.sl2", false),
        ] {
            let q = Query { name: "x".into(), source: Some(GameSource::Steam), source_key: "1".into(), install_dir: Some(t.path().join("jeu")) };
            let env = Places { app_data: Some(t.path().join("roaming")), steam_root: Some(t.path().join("steam")), ..env.clone() };
            assert_eq!(resolve(pattern, &q, &env, false, "Montagne", true).is_some(), ok, "{pattern}");
        }
        // Dossier cité par ce seul jeu dans le manifeste : propre au jeu malgré le joker.
        let q1 = Query { name: "x".into(), source: Some(GameSource::Steam), source_key: "1".into(), install_dir: None };
        let env1 = Places { app_data: Some(t.path().join("roaming")), ..env.clone() };
        assert!(resolve("<winAppData>/3Stars/*.rxdata", &q1, &env1, false, "3 Stars of Destiny", false).is_some());
        assert!(resolve("<winAppData>/3Stars/*.rxdata", &q1, &env1, false, "3 Stars of Destiny", true).is_none());
        let q = Query { name: "x".into(), source: Some(GameSource::Steam), source_key: "1".into(), install_dir: None };
        let env2 = Places { steam_root: Some(t.path().join("steam")), ..env.clone() };
        assert_eq!(resolve("<root>/userdata/<storeUserId>/525480/remote/savedata", &q, &env2, false, "Jeu", true), Some(t.path().join("steam/userdata/42/525480/remote/savedata")));
        if cfg!(target_os = "linux") {
            assert!(locs.contains(&t.path().join("data/Celeste/Saves")));
        }
        // Règle Windows d'un jeu Steam : sous Linux, cherchée dans le préfixe Proton.
        let pfx = t.path().join("steam/steamapps/compatdata/1059220/pfx/drive_c/users/steamuser");
        std::fs::create_dir_all(&pfx).unwrap();
        let env = Places { steam_root: Some(t.path().join("steam")), ..env }.with_proton("1059220", None);
        let q = Query { name: "'83".into(), source: Some(GameSource::Steam), source_key: "1059220".into(), install_dir: None };
        let locs = idx.locations(&q, &env);
        let root = if cfg!(windows) { t.path().join("local") } else { pfx.join("AppData/Local") };
        assert_eq!(locs, vec![root.join("XV83/Saved/Config/Windows"), root.join("XV83/Saved/SaveGames")]);
    }
}

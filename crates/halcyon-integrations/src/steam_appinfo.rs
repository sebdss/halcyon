//! Fiches des jeux Steam lues **hors ligne** dans le cache du client
//! (`appcache/appinfo.vdf`) : studio, éditeur, date de sortie, catégories (solo, coop,
//! manette…) et étiquettes de la boutique (« Roguelike », « Monde ouvert »…), sans requête.
//!
//! Format (versions 27, 28 et 29) : en-tête `magic, univers` (+ position de la table des
//! noms en v29), puis pour chaque application `appid, taille, en-tête fixe (40 ou 60 octets),
//! KeyValues binaire` ; `appid = 0` termine la liste. En v29, les noms des clés sont des
//! indices dans une table placée en fin de fichier. Une entrée illisible est ignorée ; un
//! fichier tronqué (Steam le réécrit) garde ce qui a pu être lu.

use crate::steam_stats::{parse_binary_at, Kv};
use std::collections::{HashMap, HashSet};
use std::path::Path;

const V27: u32 = 0x0756_4427;
const V28: u32 = 0x0756_4428;
const V29: u32 = 0x0756_4429;
/// Plafond de lecture : les très grandes bibliothèques dépassent rarement 300 Mo.
const MAX_FILE: u64 = 1024 * 1024 * 1024;

/// Ce qu'Halcyon retient d'une fiche.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AppInfo {
    pub kind: String,
    pub developer: Option<String>,
    pub publisher: Option<String>,
    /// Date de sortie (horodatage Unix).
    pub release: Option<i64>,
    /// Catégories de la boutique (1 multijoueur, 2 solo, 9 coop, 28 manette…).
    pub categories: Vec<u64>,
    /// Étiquettes de la boutique, des plus votées aux moins votées.
    pub store_tags: Vec<u64>,
    /// « full », « partial » ou rien.
    pub controller: Option<String>,
    /// Avis positifs (%) selon la boutique.
    pub review: Option<u8>,
}

fn s(kv: &Kv, key: &str) -> Option<String> {
    kv.get(key).and_then(|v| v.as_str().map(str::to_string).or_else(|| v.as_i64().map(|i| i.to_string()))).filter(|v| !v.trim().is_empty())
}

/// Champs utiles du nœud `appinfo/common`.
pub fn from_common(common: &Kv) -> AppInfo {
    let mut info = AppInfo { kind: s(common, "type").unwrap_or_default().to_lowercase(), ..Default::default() };
    // `associations` : { "0": { type: developer, name: … }, … } ; anciens champs `developer`.
    if let Some(assoc) = common.get("associations") {
        for (_, a) in assoc.entries() {
            let (Some(t), Some(n)) = (s(a, "type"), s(a, "name")) else { continue };
            match t.as_str() {
                "developer" if info.developer.is_none() => info.developer = Some(n),
                "publisher" if info.publisher.is_none() => info.publisher = Some(n),
                _ => {}
            }
        }
    }
    info.developer = info.developer.or_else(|| common.path(&["extended", "developer"]).and_then(|v| v.as_str().map(str::to_string)));
    info.publisher = info.publisher.or_else(|| common.path(&["extended", "publisher"]).and_then(|v| v.as_str().map(str::to_string)));
    info.release = ["steam_release_date", "original_release_date"]
        .iter()
        .find_map(|k| common.get(k).and_then(Kv::as_i64))
        .filter(|&t| t > 0);
    if let Some(cat) = common.get("category") {
        info.categories = cat.entries().iter().filter_map(|(k, _)| k.strip_prefix("category_")?.parse().ok()).collect();
    }
    if let Some(tags) = common.get("store_tags") {
        let mut t: Vec<(u64, u64)> = tags.entries().iter().filter_map(|(k, v)| Some((k.parse().ok()?, v.as_i64()? as u64))).collect();
        t.sort();
        info.store_tags = t.into_iter().map(|(_, id)| id).collect();
    }
    info.controller = s(common, "controller_support").map(|c| c.to_lowercase()).filter(|c| c == "full" || c == "partial");
    info.review = common.get("review_percentage").and_then(Kv::as_i64).filter(|r| (0..=100).contains(r)).map(|r| r as u8);
    info
}

/// Lit les fiches des applications demandées.
pub fn read(path: &Path, wanted: &HashSet<u64>) -> Result<HashMap<u64, AppInfo>, String> {
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if meta.len() > MAX_FILE {
        return Err(tr!("{path} : fichier trop volumineux", path = path.display()));
    }
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    parse(&data, wanted)
}

fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(data.get(at..at + 4)?.try_into().ok()?))
}

/// Table des noms (v29) : `nombre, puis chaînes terminées par 0`.
fn string_table(data: &[u8], at: usize) -> Option<Vec<String>> {
    let count = u32_at(data, at)? as usize;
    let mut pos = at + 4;
    let mut out = Vec::with_capacity(count.min(1 << 20));
    for _ in 0..count {
        let rest = data.get(pos..)?;
        let end = rest.iter().position(|&b| b == 0)?;
        out.push(String::from_utf8_lossy(&rest[..end]).into_owned());
        pos += end + 1;
    }
    Some(out)
}

pub fn parse(data: &[u8], wanted: &HashSet<u64>) -> Result<HashMap<u64, AppInfo>, String> {
    let magic = u32_at(data, 0).ok_or_else(|| tr!("fichier Steam incomplet"))?;
    let (header, mut pos, strings) = match magic {
        V27 => (40, 8, None),
        V28 => (60, 8, None),
        V29 => {
            let at = data.get(8..16).and_then(|b| b.try_into().ok()).map(i64::from_le_bytes).ok_or_else(|| tr!("fichier Steam incomplet"))?;
            let table = usize::try_from(at).ok().and_then(|at| string_table(data, at)).ok_or_else(|| tr!("fichier Steam incomplet"))?;
            (60, 16, Some(table))
        }
        other => return Err(tr!("fichier Steam d'un format inconnu (type {kind})", kind = format!("{other:#010x}"))),
    };
    let mut out = HashMap::new();
    while let (Some(appid), Some(size)) = (u32_at(data, pos), u32_at(data, pos + 4)) {
        if appid == 0 {
            break;
        }
        let start = pos + 8;
        let end = start + size as usize;
        if end > data.len() || (size as usize) < header {
            break; // fichier tronqué : on garde ce qui précède
        }
        if wanted.contains(&u64::from(appid)) {
            let mut at = start + header;
            if let Ok(kv) = parse_binary_at(&data[..end], &mut at, strings.as_deref()) {
                if let Some(common) = kv.path(&["appinfo", "common"]) {
                    out.insert(u64::from(appid), from_common(common));
                }
            }
        }
        pos = end;
    }
    Ok(out)
}

/// Étiquettes de la boutique : identifiant → nom dans la langue de Halcyon, d'après
/// `IStoreService/GetTagList` (public, sans clé), mis en cache.
pub fn parse_tag_list(v: &serde_json::Value) -> HashMap<u64, String> {
    v.pointer("/response/tags")
        .and_then(|t| t.as_array())
        .map(|a| a.iter().filter_map(|t| Some((t.get("tagid")?.as_u64()?, t.get("name")?.as_str()?.trim().to_string()))).filter(|(_, n)| !n.is_empty()).collect())
        .unwrap_or_default()
}

pub fn tag_list_url(language: &str) -> String {
    format!("https://api.steampowered.com/IStoreService/GetTagList/v1/?language={}", crate::plugins::encode(language))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::steam_stats::write_binary;

    fn m(items: Vec<(&str, Kv)>) -> Kv {
        Kv::Map(items.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
    }
    fn st(v: &str) -> Kv {
        Kv::Str(v.into())
    }

    fn common() -> Kv {
        m(vec![(
            "appinfo",
            m(vec![
                ("appid", Kv::Int(1145360)),
                (
                    "common",
                    m(vec![
                        ("name", st("Hades")),
                        ("type", st("Game")),
                        ("associations", m(vec![("0", m(vec![("type", st("developer")), ("name", st("Supergiant Games"))])), ("1", m(vec![("type", st("publisher")), ("name", st("Supergiant Games"))]))])),
                        ("steam_release_date", Kv::Int(1_600_300_800)),
                        ("category", m(vec![("category_2", st("1")), ("category_28", st("1"))])),
                        ("store_tags", m(vec![("1", Kv::Int(3959)), ("0", Kv::Int(1716))])),
                        ("controller_support", st("full")),
                        ("review_percentage", Kv::Int(98)),
                    ]),
                ),
            ]),
        )])
    }

    /// Fichier v28 : en-tête 60 octets, noms en clair.
    fn v28(apps: &[(u32, &Kv)]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend(V28.to_le_bytes());
        out.extend(1u32.to_le_bytes());
        for (id, kv) in apps {
            let body = write_binary(kv);
            out.extend(id.to_le_bytes());
            out.extend(((60 + body.len()) as u32).to_le_bytes());
            out.extend([0u8; 60]);
            out.extend(body);
        }
        out.extend(0u32.to_le_bytes());
        out
    }

    #[test]
    fn reads_v28_entries() {
        let other = m(vec![("appinfo", m(vec![("common", m(vec![("name", st("Outil"))]))]))]);
        let data = v28(&[(7, &other), (1145360, &common())]);
        let got = parse(&data, &[1145360, 99].into_iter().collect()).unwrap();
        assert_eq!(got.len(), 1);
        let h = &got[&1145360];
        assert_eq!((h.kind.as_str(), h.developer.as_deref(), h.publisher.as_deref()), ("game", Some("Supergiant Games"), Some("Supergiant Games")));
        assert_eq!(h.release, Some(1_600_300_800));
        assert_eq!(h.categories.iter().copied().collect::<HashSet<_>>(), [2, 28].into_iter().collect());
        assert_eq!(h.store_tags, [1716, 3959], "ordre de la boutique");
        assert_eq!((h.controller.as_deref(), h.review), (Some("full"), Some(98)));
        // Fichier tronqué : ce qui précède reste lu.
        let cut = &data[..data.len() - 30];
        assert!(parse(cut, &[1145360].into_iter().collect()).unwrap().is_empty());
        assert!(parse(&[1, 2, 3, 4, 0, 0, 0, 0], &HashSet::new()).is_err());
    }

    #[test]
    fn reads_v29_string_table() {
        // Noms remplacés par des indices ; table en fin de fichier.
        let names = ["appinfo", "common", "name", "type", "developer", "associations", "0", "steam_release_date"];
        fn encode(kv: &Kv, names: &[&str], out: &mut Vec<u8>) {
            for (k, v) in kv.entries() {
                let idx = names.iter().position(|n| n == k).unwrap() as u32;
                match v {
                    Kv::Map(_) => {
                        out.push(0);
                        out.extend(idx.to_le_bytes());
                        encode(v, names, out);
                        out.push(8);
                    }
                    Kv::Str(s) => {
                        out.push(1);
                        out.extend(idx.to_le_bytes());
                        out.extend(s.as_bytes());
                        out.push(0);
                    }
                    Kv::Int(i) => {
                        out.push(2);
                        out.extend(idx.to_le_bytes());
                        out.extend((*i as i32).to_le_bytes());
                    }
                    Kv::Float(_) => unreachable!(),
                }
            }
        }
        let kv = m(vec![(
            "appinfo",
            m(vec![("common", m(vec![("name", st("Celeste")), ("type", st("Game")), ("associations", m(vec![("0", m(vec![("type", st("developer")), ("name", st("Maddy Makes Games"))]))])), ("steam_release_date", Kv::Int(1_516_000_000))]))]),
        )]);
        let mut body = Vec::new();
        encode(&kv, &names, &mut body);
        body.push(8);
        let mut out = Vec::new();
        out.extend(V29.to_le_bytes());
        out.extend(1u32.to_le_bytes());
        let table_at_pos = out.len();
        out.extend(0i64.to_le_bytes());
        out.extend(504230u32.to_le_bytes());
        out.extend(((60 + body.len()) as u32).to_le_bytes());
        out.extend([0u8; 60]);
        out.extend(&body);
        out.extend(0u32.to_le_bytes());
        let table_at = out.len() as i64;
        out[table_at_pos..table_at_pos + 8].copy_from_slice(&table_at.to_le_bytes());
        out.extend((names.len() as u32).to_le_bytes());
        for n in names {
            out.extend(n.as_bytes());
            out.push(0);
        }
        let got = parse(&out, &[504230].into_iter().collect()).unwrap();
        assert_eq!(got[&504230].developer.as_deref(), Some("Maddy Makes Games"));
        assert_eq!(got[&504230].release, Some(1_516_000_000));
    }

    #[test]
    fn tag_list() {
        let v = serde_json::json!({"response": {"tags": [{"tagid": 1716, "name": "Rogue-like"}, {"tagid": 9, "name": " "}]}});
        let t = parse_tag_list(&v);
        assert_eq!(t.get(&1716).map(String::as_str), Some("Rogue-like"));
        assert!(!t.contains_key(&9));
    }
}

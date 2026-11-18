//! Succès Steam lus **sur le disque**, dans le cache du client Steam : sans clé d'API, sans
//! profil public, sans connexion.
//!
//! - `appcache/stats/UserGameStatsSchema_<appid>.bin` : définition des succès (noms et
//!   descriptions traduits, icônes, succès cachés), par statistique de type « bits » ;
//! - `appcache/stats/UserGameStats_<compte>_<appid>.bin` : progression du compte, avec
//!   `cache/<statistique>/data` (un bit par succès débloqué) et
//!   `cache/<statistique>/AchievementTimes/<bit>` (date de déblocage).
//!
//! Les deux fichiers sont au format KeyValues binaire de Valve. Le client Steam les écrit pour
//! les jeux dont il a chargé les statistiques sur ce PC (en pratique : les jeux lancés ici).
//! Le format n'est pas documenté publiquement ; la lecture suit celle de Steam Achievement
//! Manager. Un fichier inattendu est ignoré (`Err`), jamais interprété au hasard.

use halcyon_core::achievements::AchievementData;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Adresse publique des icônes de succès (le schéma ne donne que le nom de fichier).
pub const ICON_CDN: &str = "https://cdn.steamstatic.com/steamcommunity/public/images/apps";
/// Écart entre un SteamID 64 bits et le numéro de compte des noms de fichiers.
const STEAMID64_BASE: u64 = 76_561_197_960_265_728;
/// Taille maximale lue (un schéma de plusieurs centaines de succès fait quelques centaines de Ko).
const MAX_FILE: u64 = 32 * 1024 * 1024;

// ------------------------------------------------------------------ KeyValues binaire

/// Nœud d'un fichier KeyValues binaire.
#[derive(Clone, Debug, PartialEq)]
pub enum Kv {
    Map(Vec<(String, Kv)>),
    Str(String),
    Int(i64),
    Float(f32),
}

impl Kv {
    /// Enfant par nom (sans tenir compte de la casse, comme Steam).
    pub fn get(&self, key: &str) -> Option<&Kv> {
        match self {
            Kv::Map(items) => items.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn path(&self, keys: &[&str]) -> Option<&Kv> {
        keys.iter().try_fold(self, |node, k| node.get(k))
    }

    pub fn entries(&self) -> &[(String, Kv)] {
        match self {
            Kv::Map(items) => items,
            _ => &[],
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Kv::Str(s) => Some(s),
            _ => None,
        }
    }

    /// Entier, qu'il soit stocké comme nombre ou comme texte (« 4 », « 1 »).
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Kv::Int(i) => Some(*i),
            Kv::Float(f) => Some(*f as i64),
            Kv::Str(s) => s.trim().parse().ok(),
            Kv::Map(_) => None,
        }
    }
}

/// Lit un document KeyValues binaire : suite de `type, nom\0, valeur`, terminée par 0x08.
pub fn parse_binary(data: &[u8]) -> Result<Kv, String> {
    let mut pos = 0;
    let root = read_map(data, &mut pos, 0)?;
    Ok(root)
}

fn read_map(data: &[u8], pos: &mut usize, depth: usize) -> Result<Kv, String> {
    if depth > 64 {
        return Err(tr!("fichier Steam illisible (imbrication trop profonde)"));
    }
    let mut items = Vec::new();
    loop {
        let Some(&kind) = data.get(*pos) else {
            // Fin de fichier sans marqueur : accepté seulement à la racine.
            return if depth == 0 { Ok(Kv::Map(items)) } else { Err(tr!("fichier Steam incomplet")) };
        };
        *pos += 1;
        // 0x08 : fin de bloc ; 0x0B : variante de fin rencontrée dans certains fichiers.
        if kind == 0x08 || kind == 0x0B {
            return Ok(Kv::Map(items));
        }
        let name = read_cstr(data, pos)?;
        let value = match kind {
            0x00 => read_map(data, pos, depth + 1)?,
            0x01 => Kv::Str(read_cstr(data, pos)?),
            0x05 => Kv::Str(read_wstr(data, pos)?),
            0x02 | 0x04 | 0x06 => Kv::Int(i64::from(i32::from_le_bytes(take::<4>(data, pos)?))),
            0x03 => Kv::Float(f32::from_le_bytes(take::<4>(data, pos)?)),
            0x07 => Kv::Int(u64::from_le_bytes(take::<8>(data, pos)?) as i64),
            0x0A => Kv::Int(i64::from_le_bytes(take::<8>(data, pos)?)),
            other => return Err(tr!("fichier Steam d'un format inconnu (type {kind})", kind = format!("{other:#04x}"))),
        };
        items.push((name, value));
    }
}

fn read_cstr(data: &[u8], pos: &mut usize) -> Result<String, String> {
    let rest = data.get(*pos..).ok_or_else(|| tr!("fichier Steam incomplet"))?;
    let end = rest.iter().position(|&b| b == 0).ok_or_else(|| tr!("fichier Steam incomplet"))?;
    let s = String::from_utf8_lossy(&rest[..end]).into_owned();
    *pos += end + 1;
    Ok(s)
}

/// Chaîne UTF-16 (petit-boutiste) terminée par deux octets nuls.
fn read_wstr(data: &[u8], pos: &mut usize) -> Result<String, String> {
    let rest = data.get(*pos..).ok_or_else(|| tr!("fichier Steam incomplet"))?;
    let units: Vec<u16> = rest.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).take_while(|&u| u != 0).collect();
    if units.len() * 2 + 2 > rest.len() {
        return Err(tr!("fichier Steam incomplet"));
    }
    *pos += units.len() * 2 + 2;
    Ok(String::from_utf16_lossy(&units))
}

fn take<const N: usize>(data: &[u8], pos: &mut usize) -> Result<[u8; N], String> {
    let bytes = data.get(*pos..*pos + N).ok_or_else(|| tr!("fichier Steam incomplet"))?;
    *pos += N;
    Ok(bytes.try_into().expect("taille vérifiée"))
}

// ------------------------------------------------------------------ succès

/// Numéro de compte (SteamID3) à partir d'un SteamID 64 bits.
pub fn account_id(steam_id64: u64) -> u32 {
    steam_id64.checked_sub(STEAMID64_BASE).unwrap_or(steam_id64) as u32
}

pub fn schema_path(steam_root: &Path, appid: u64) -> PathBuf {
    steam_root.join("appcache").join("stats").join(format!("UserGameStatsSchema_{appid}.bin"))
}

pub fn progress_path(steam_root: &Path, account: u32, appid: u64) -> PathBuf {
    steam_root.join("appcache").join("stats").join(format!("UserGameStats_{account}_{appid}.bin"))
}

/// Date de dernière modification de la progression locale (pour ne relire que ce qui change).
pub fn progress_modified(steam_root: &Path, account: u32, appid: u64) -> Option<i64> {
    let t = std::fs::metadata(progress_path(steam_root, account, appid)).ok()?.modified().ok()?;
    t.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs() as i64)
}

/// Une définition de succès lue dans le schéma.
#[derive(Clone, Debug, PartialEq)]
struct Def {
    stat: String,
    bit: u32,
    data: AchievementData,
    icon: String,
    icon_gray: String,
}

/// Texte traduit : langue demandée, sinon anglais, sinon valeur simple, sinon `fallback`.
fn localized(node: Option<&Kv>, language: &str, fallback: &str) -> String {
    let pick = |n: &Kv| n.as_str().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    node.and_then(|n| n.get(language).and_then(pick).or_else(|| n.get("english").and_then(pick)).or_else(|| pick(n)))
        .unwrap_or_else(|| fallback.to_string())
}

/// Lit les définitions des succès. `language` : nom de langue Steam (« french », « german »…).
fn parse_schema(schema: &Kv, appid: u64, language: &str) -> Vec<Def> {
    let Some(stats) = schema.get(&appid.to_string()).and_then(|g| g.get("stats")).or_else(|| {
        // Racine sans numéro de jeu attendu : premier nœud qui contient « stats ».
        schema.entries().iter().find_map(|(_, v)| v.get("stats"))
    }) else {
        return Vec::new();
    };
    let mut defs = Vec::new();
    for (stat_key, stat) in stats.entries() {
        // Type 4 : succès ; 5 : succès groupés. Ancien format : « type_int ».
        let kind = stat.get("type_int").or_else(|| stat.get("type")).and_then(|t| {
            t.as_i64().or_else(|| match t.as_str()?.to_ascii_lowercase().as_str() {
                "achievements" => Some(4),
                "groupachievements" => Some(5),
                _ => None,
            })
        });
        // Type absent : la présence de « bits » suffit (comme Steam Achievement Manager et
        // d'autres lecteurs) ; un autre type déclaré (entier, flottant…) n'est pas un succès.
        if kind.is_some_and(|k| k != 4 && k != 5) {
            continue;
        }
        let stat_id = stat.get("id").and_then(Kv::as_i64).map(|i| i.to_string()).unwrap_or_else(|| stat_key.clone());
        let Some(bits) = stat.get("bits") else { continue };
        for (bit_key, bit) in bits.entries() {
            let Some(index) = bit.get("bit").and_then(Kv::as_i64).or_else(|| bit_key.parse().ok()) else { continue };
            let Some(api) = bit.get("name").and_then(Kv::as_str).map(str::to_string).filter(|s| !s.is_empty()) else { continue };
            let display = bit.get("display");
            let field = |k: &str| display.and_then(|d| d.get(k)).and_then(Kv::as_str).unwrap_or_default().to_string();
            defs.push(Def {
                stat: stat_id.clone(),
                bit: index as u32,
                icon: field("icon"),
                icon_gray: field("icon_gray"),
                data: AchievementData {
                    name: localized(display.and_then(|d| d.get("name")), language, &api),
                    description: localized(display.and_then(|d| d.get("desc")), language, ""),
                    hidden: display.and_then(|d| d.get("hidden")).and_then(Kv::as_i64).unwrap_or(0) != 0,
                    api_name: api,
                    icon_url: None,
                    points: None,
                    rarity: None,
                    unlocked: false,
                    unlocked_at: None,
                    unlocked_hardcore_at: None,
                    progress: None,
                    display_order: 0,
                },
            });
        }
    }
    // Ordre d'affichage : celui du schéma (statistique, puis bit).
    defs.sort_by(|a, b| (a.stat.parse::<i64>().unwrap_or(i64::MAX), a.bit).cmp(&(b.stat.parse::<i64>().unwrap_or(i64::MAX), b.bit)));
    defs
}

/// Bits débloqués et dates, par statistique, lus dans la progression du compte.
fn parse_progress(progress: &Kv) -> BTreeMap<String, (u32, BTreeMap<u32, i64>)> {
    let cache = progress.get("cache").unwrap_or(progress);
    let mut out = BTreeMap::new();
    for (key, stat) in cache.entries() {
        if !matches!(stat, Kv::Map(_)) {
            continue;
        }
        let bits = stat.get("data").and_then(Kv::as_i64).unwrap_or(0) as u32;
        let times = stat
            .get("AchievementTimes")
            .map(|t| t.entries().iter().filter_map(|(b, v)| Some((b.parse().ok()?, v.as_i64()?))).collect())
            .unwrap_or_default();
        out.insert(key.clone(), (bits, times));
    }
    out
}

/// Assemble définitions et progression en liste de succès.
fn combine(appid: u64, defs: Vec<Def>, progress: Option<&Kv>) -> Vec<AchievementData> {
    let state = progress.map(parse_progress).unwrap_or_default();
    defs.into_iter()
        .enumerate()
        .map(|(i, d)| {
            let (bits, times) = state.get(&d.stat).cloned().unwrap_or_default();
            let time = times.get(&d.bit).copied().filter(|&t| t > 0);
            let unlocked = d.bit < 32 && (bits >> d.bit) & 1 == 1 || time.is_some();
            let icon = if unlocked || d.icon_gray.is_empty() { &d.icon } else { &d.icon_gray };
            AchievementData {
                unlocked,
                unlocked_at: time.filter(|_| unlocked),
                icon_url: (!icon.is_empty()).then(|| format!("{ICON_CDN}/{appid}/{icon}")),
                display_order: i as i64,
                ..d.data
            }
        })
        .collect()
}

fn read_kv(path: &Path) -> Result<Option<Kv>, String> {
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    if meta.len() > MAX_FILE {
        return Err(tr!("{path} : fichier trop volumineux", path = path.display()));
    }
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    parse_binary(&data).map(Some).map_err(|e| format!("{} : {e}", path.display()))
}

/// Succès d'un jeu Steam d'après le cache du client.
///
/// - `Ok(None)` : Steam n'a pas encore enregistré les succès de ce jeu sur ce PC ;
/// - `Ok(Some(vec![]))` : le jeu n'a pas de succès ;
/// - sans fichier de progression, tous les succès sont verrouillés.
pub fn local_achievements(steam_root: &Path, account: u32, appid: u64, language: &str) -> Result<Option<Vec<AchievementData>>, String> {
    let Some(schema) = read_kv(&schema_path(steam_root, appid))? else { return Ok(None) };
    let defs = parse_schema(&schema, appid, language);
    if defs.is_empty() {
        return Ok(Some(Vec::new()));
    }
    let progress = read_kv(&progress_path(steam_root, account, appid))?;
    Ok(Some(combine(appid, defs, progress.as_ref())))
}

// ------------------------------------------------------------------ écriture (tests)

/// Écrit un document KeyValues binaire (tests et outils ; Halcyon n'écrit jamais dans Steam).
pub fn write_binary(kv: &Kv) -> Vec<u8> {
    fn map(out: &mut Vec<u8>, items: &[(String, Kv)]) {
        for (k, v) in items {
            let (kind, body): (u8, Vec<u8>) = match v {
                Kv::Map(inner) => {
                    let mut b = Vec::new();
                    map(&mut b, inner);
                    b.push(0x08);
                    (0x00, b)
                }
                Kv::Str(s) => {
                    let mut b = s.as_bytes().to_vec();
                    b.push(0);
                    (0x01, b)
                }
                Kv::Int(i) if i32::try_from(*i).is_ok() => (0x02, (*i as i32).to_le_bytes().to_vec()),
                Kv::Int(i) => (0x07, (*i as u64).to_le_bytes().to_vec()),
                Kv::Float(f) => (0x03, f.to_le_bytes().to_vec()),
            };
            out.push(kind);
            out.extend_from_slice(k.as_bytes());
            out.push(0);
            out.extend(body);
        }
    }
    let mut out = Vec::new();
    map(&mut out, kv.entries());
    out.push(0x08);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(items: Vec<(&str, Kv)>) -> Kv {
        Kv::Map(items.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
    }
    fn s(v: &str) -> Kv {
        Kv::Str(v.into())
    }

    /// Schéma au format observé dans Steam : jeu › stats › statistique de type 4 › bits.
    fn schema(appid: u64) -> Kv {
        let bit = |n: &str, api: &str, en: &str, fr: Option<&str>, hidden: &str| -> (String, Kv) {
            let mut name = vec![("english", s(en)), ("token", s("NEW_ACHIEVEMENT_1_0_NAME"))];
            if let Some(fr) = fr {
                name.insert(0, ("french", s(fr)));
            }
            (
                n.to_string(),
                m(vec![
                    ("name", s(api)),
                    ("bit", Kv::Int(n.parse().unwrap())),
                    (
                        "display",
                        m(vec![
                            ("name", m(name)),
                            ("desc", m(vec![("english", s(&format!("{en} desc")))])),
                            ("hidden", s(hidden)),
                            ("icon", s(&format!("{api}.jpg"))),
                            ("icon_gray", s(&format!("{api}_gray.jpg"))),
                        ]),
                    ),
                ]),
            )
        };
        m(vec![(
            &appid.to_string(),
            m(vec![
                ("gamename", s("Jeu")),
                ("version", s("12")),
                (
                    "stats",
                    m(vec![
                        // statistique ordinaire, ignorée
                        ("2", m(vec![("type", s("1")), ("id", s("2")), ("name", s("kills"))])),
                        (
                            "1",
                            m(vec![
                                ("type", s("4")),
                                ("id", s("1")),
                                (
                                    "bits",
                                    Kv::Map(vec![
                                        bit("0", "ACH_FIRST", "First", Some("Premier"), "0"),
                                        bit("1", "ACH_SECOND", "Second", None, "0"),
                                        bit("2", "ACH_SECRET", "Secret", None, "1"),
                                    ]),
                                ),
                            ]),
                        ),
                    ]),
                ),
            ]),
        )])
    }

    fn progress(bits: i64, times: Vec<(&str, i64)>) -> Kv {
        m(vec![(
            "cache",
            m(vec![
                ("crc", Kv::Int(12345)),
                ("PendingChanges", Kv::Int(0)),
                ("1", m(vec![("data", Kv::Int(bits)), ("AchievementTimes", m(times.into_iter().map(|(k, v)| (k, Kv::Int(v))).collect()))])),
            ]),
        )])
    }

    #[test]
    fn binary_round_trip() {
        let doc = schema(620);
        let bytes = write_binary(&doc);
        assert_eq!(parse_binary(&bytes).unwrap(), doc);
        assert!(parse_binary(&bytes[..bytes.len() / 2]).is_err(), "fichier tronqué refusé");
        assert!(parse_binary(&[0x09, b'a', 0]).is_err(), "type inconnu refusé");
    }

    #[test]
    fn reads_local_achievements() {
        let dir = tempfile::tempdir().unwrap();
        let stats = dir.path().join("appcache/stats");
        std::fs::create_dir_all(&stats).unwrap();
        let account = account_id(76_561_198_000_000_042);
        assert_eq!(account, 39_734_314);
        assert_eq!(local_achievements(dir.path(), account, 620, "french").unwrap(), None, "pas de cache : rien");

        std::fs::write(schema_path(dir.path(), 620), write_binary(&schema(620))).unwrap();
        let locked = local_achievements(dir.path(), account, 620, "french").unwrap().unwrap();
        assert_eq!(locked.len(), 3);
        assert!(locked.iter().all(|a| !a.unlocked), "sans progression, tout est verrouillé");
        assert_eq!(locked[0].icon_url.as_deref(), Some("https://cdn.steamstatic.com/steamcommunity/public/images/apps/620/ACH_FIRST_gray.jpg"));

        // bits 0 et 2 débloqués ; date connue pour le bit 0 seulement
        std::fs::write(progress_path(dir.path(), account, 620), write_binary(&progress(0b101, vec![("0", 1_700_000_000)]))).unwrap();
        let list = local_achievements(dir.path(), account, 620, "french").unwrap().unwrap();
        let by = |api: &str| list.iter().find(|a| a.api_name == api).unwrap();
        assert_eq!(by("ACH_FIRST").name, "Premier", "nom traduit");
        assert_eq!(by("ACH_SECOND").name, "Second", "repli sur l'anglais");
        assert_eq!(by("ACH_FIRST").description, "First desc");
        assert!(by("ACH_FIRST").unlocked && by("ACH_SECRET").unlocked && !by("ACH_SECOND").unlocked);
        assert_eq!(by("ACH_FIRST").unlocked_at, Some(1_700_000_000));
        assert_eq!(by("ACH_SECRET").unlocked_at, None);
        assert!(by("ACH_SECRET").hidden);
        assert_eq!(by("ACH_FIRST").icon_url.as_deref(), Some("https://cdn.steamstatic.com/steamcommunity/public/images/apps/620/ACH_FIRST.jpg"));
        assert_eq!(list.iter().map(|a| a.display_order).collect::<Vec<_>>(), [0, 1, 2]);
    }

    #[test]
    fn game_without_achievements_and_old_schema_format() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("appcache/stats")).unwrap();
        let no_ach = m(vec![("10", m(vec![("stats", m(vec![("1", m(vec![("type", s("1")), ("name", s("x"))]))]))]))]);
        std::fs::write(schema_path(dir.path(), 10), write_binary(&no_ach)).unwrap();
        assert_eq!(local_achievements(dir.path(), 1, 10, "english").unwrap(), Some(vec![]));

        // ancien format : « type_int » numérique, nom de statistique comme clé
        let old = m(vec![(
            "20",
            m(vec![(
                "stats",
                m(vec![(
                    "3",
                    m(vec![
                        ("type_int", Kv::Int(4)),
                        ("bits", m(vec![("0", m(vec![("name", s("A")), ("display", m(vec![("name", s("Alpha"))]))]))])),
                    ]),
                )]),
            )]),
        )]);
        std::fs::write(schema_path(dir.path(), 20), write_binary(&old)).unwrap();
        let p = m(vec![("cache", m(vec![("3", m(vec![("data", Kv::Int(1))]))]))]);
        std::fs::write(progress_path(dir.path(), 1, 20), write_binary(&p)).unwrap();
        let list = local_achievements(dir.path(), 1, 20, "french").unwrap().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "Alpha", "nom simple sans traductions");
        assert!(list[0].unlocked);
    }

    #[test]
    fn reads_wide_strings_and_alternate_end_marker() {
        // racine { "a" (UTF-16) = "é1", "b" = { } terminé par 0x0B }
        let mut data = vec![0x05];
        data.extend(b"a\0");
        data.extend([0xE9, 0x00, 0x31, 0x00, 0x00, 0x00]);
        data.push(0x00);
        data.extend(b"b\0");
        data.push(0x0B);
        data.push(0x08);
        let kv = parse_binary(&data).unwrap();
        assert_eq!(kv.get("a").and_then(Kv::as_str), Some("é1"));
        assert!(matches!(kv.get("b"), Some(Kv::Map(m)) if m.is_empty()));
        assert!(parse_binary(&[0x05, b'a', 0, 0x41]).is_err());
    }

}

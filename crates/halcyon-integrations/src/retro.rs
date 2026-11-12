//! RetroAchievements (fonctions **en ligne**, nom d'utilisateur + clé Web API du compte).
//!
//! - identification d'une ROM : empreinte (`rahash`) comparée à la liste des
//!   empreintes de la console (`API_GetGameList`, mise en cache 7 jours) ;
//! - progression : `API_GetGameInfoAndUserProgress` (modes normal et Hardcore).

use crate::artwork::{Downloader, FetchError};
use halcyon_core::achievements::AchievementData;
use halcyon_core::util::parse_datetime;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

const API: &str = "https://retroachievements.org/API";
const MEDIA: &str = "https://media.retroachievements.org";
const CACHE_SECS: u64 = 7 * 86_400;

#[derive(Clone, Debug)]
pub struct RetroAchievements {
    pub user: String,
    pub key: String,
}

/// Progression d'un jeu : succès + jaquette éventuelle.
#[derive(Clone, Debug, Default)]
pub struct GameProgress {
    pub achievements: Vec<AchievementData>,
    pub box_art: Option<String>,
    pub title: String,
}

impl RetroAchievements {
    fn auth(&self) -> String {
        format!("y={}&u={}", enc(&self.key), enc(&self.user))
    }

    /// Table empreinte → identifiant de jeu pour une console (cache disque).
    pub fn hash_table(&self, dl: &Downloader, console: u32, cache_dir: &Path) -> Result<HashMap<String, u32>, FetchError> {
        let cache = cache_dir.join(format!("ra-console-{console}.json"));
        let fresh = std::fs::metadata(&cache)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age.as_secs() < CACHE_SECS);
        let json = match (fresh, std::fs::read(&cache)) {
            (true, Ok(bytes)) => serde_json::from_slice(&bytes).unwrap_or(Value::Null),
            _ => {
                let v = dl.get_json(&format!("{API}/API_GetGameList.php?{}&i={console}&h=1&f=1", self.auth()))?;
                let _ = std::fs::create_dir_all(cache_dir);
                let _ = std::fs::write(&cache, v.to_string());
                v
            }
        };
        Ok(parse_hash_table(&json))
    }

    pub fn progress(&self, dl: &Downloader, game_id: u32) -> Result<GameProgress, FetchError> {
        let v = dl.get_json(&format!("{API}/API_GetGameInfoAndUserProgress.php?{}&g={game_id}", self.auth()))?;
        Ok(parse_progress(&v))
    }
}

fn enc(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

pub fn parse_hash_table(v: &Value) -> HashMap<String, u32> {
    let mut out = HashMap::new();
    for game in v.as_array().into_iter().flatten() {
        let Some(id) = game.get("ID").and_then(|x| x.as_u64().or_else(|| x.as_str()?.parse().ok())) else { continue };
        for h in game.get("Hashes").and_then(Value::as_array).into_iter().flatten() {
            if let Some(h) = h.as_str() {
                out.insert(h.to_lowercase(), id as u32);
            }
        }
    }
    out
}

fn num(v: Option<&Value>) -> Option<f64> {
    let v = v?;
    v.as_f64().or_else(|| v.as_str()?.parse().ok())
}

pub fn parse_progress(v: &Value) -> GameProgress {
    let players = num(v.get("NumDistinctPlayersCasual")).or_else(|| num(v.get("NumDistinctPlayers"))).filter(|&n| n > 0.0);
    let mut list: Vec<AchievementData> = v
        .get("Achievements")
        .and_then(Value::as_object)
        .map(|map| {
            map.values()
                .filter_map(|a| {
                    let id = num(a.get("ID"))? as u64;
                    let s = |k: &str| a.get(k).and_then(Value::as_str).map(str::to_string);
                    let earned = s("DateEarned").and_then(|d| parse_datetime(&d));
                    let hardcore = s("DateEarnedHardcore").and_then(|d| parse_datetime(&d));
                    let badge = s("BadgeName");
                    Some(AchievementData {
                        api_name: id.to_string(),
                        name: s("Title").unwrap_or_default(),
                        description: s("Description").unwrap_or_default(),
                        icon_url: badge.map(|b| {
                            if earned.is_some() || hardcore.is_some() {
                                format!("{MEDIA}/Badge/{b}.png")
                            } else {
                                format!("{MEDIA}/Badge/{b}_lock.png")
                            }
                        }),
                        hidden: false,
                        points: num(a.get("Points")).map(|p| p as u32),
                        rarity: players.and_then(|p| num(a.get("NumAwarded")).map(|n| (n * 100.0 / p) as f32)),
                        unlocked: earned.is_some() || hardcore.is_some(),
                        unlocked_at: earned.or(hardcore),
                        unlocked_hardcore_at: hardcore,
                        progress: None,
                        display_order: num(a.get("DisplayOrder")).unwrap_or(0.0) as i64,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    list.sort_by_key(|a| (a.display_order, a.api_name.parse::<u64>().unwrap_or(0)));
    GameProgress {
        achievements: list,
        box_art: v.get("ImageBoxArt").and_then(Value::as_str).filter(|s| !s.is_empty()).map(|p| format!("{MEDIA}{p}")),
        title: v.get("Title").and_then(Value::as_str).unwrap_or_default().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_progress_with_hardcore_and_rarity() {
        let v = json!({
            "Title": "Dragster", "ConsoleID": 25, "ImageBoxArt": "/Images/026766.png",
            "NumDistinctPlayersCasual": 200,
            "Achievements": {
                "79434": {"ID": 79434, "NumAwarded": 50, "Title": "Novice Dragster Driver 1",
                    "Description": "Complete your very first race", "Points": 1, "BadgeName": "85541",
                    "DisplayOrder": 0, "DateEarned": "2022-08-23 22:56:38", "DateEarnedHardcore": "2022-08-23 22:56:38"},
                "79435": {"ID": "79435", "NumAwarded": "10", "Title": "Pro", "Description": "Win", "Points": "5",
                    "BadgeName": "85542", "DisplayOrder": "1"}
            },
            "NumAwardedToUser": 1
        });
        let p = parse_progress(&v);
        assert_eq!(p.title, "Dragster");
        assert_eq!(p.box_art.as_deref(), Some("https://media.retroachievements.org/Images/026766.png"));
        assert_eq!(p.achievements.len(), 2);
        let a = &p.achievements[0];
        assert!(a.unlocked);
        assert_eq!(a.unlocked_hardcore_at, Some(1_661_295_398));
        assert_eq!(a.rarity, Some(25.0));
        assert_eq!(a.icon_url.as_deref(), Some("https://media.retroachievements.org/Badge/85541.png"));
        let b = &p.achievements[1];
        assert!(!b.unlocked);
        assert_eq!(b.points, Some(5));
        assert!(b.icon_url.as_deref().unwrap().ends_with("85542_lock.png"));
    }

    #[test]
    fn builds_hash_table_and_encodes() {
        let v = json!([{"Title": "A", "ID": 1, "Hashes": ["ABC", "def"]}, {"Title": "B", "ID": "2", "Hashes": []}]);
        let t = parse_hash_table(&v);
        assert_eq!(t.get("abc"), Some(&1));
        assert_eq!(t.get("def"), Some(&1));
        assert_eq!(enc("a b&c"), "a%20b%26c");
    }
}

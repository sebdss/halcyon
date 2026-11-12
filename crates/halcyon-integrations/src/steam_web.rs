//! Steam Web API (fonctions **en ligne**, nécessitant une clé personnelle) et
//! métadonnées publiques de la boutique Steam.
//!
//! - succès : `GetSchemaForGame` (noms, icônes) + `GetPlayerAchievements` (déblocages)
//!   + `GetGlobalAchievementPercentagesForApp` (rareté) ;
//! - jeux possédés : `GetOwnedGames` ;
//! - boutique : `store.steampowered.com/api/appdetails` (sans clé).
//!
//! Le profil Steam doit exposer publiquement les « détails du jeu » pour que les
//! déblocages soient lisibles : sinon l'erreur est remontée telle quelle.

use crate::artwork::{Downloader, FetchError};
use halcyon_core::achievements::AchievementData;
use halcyon_core::util::strip_html;
use halcyon_core::OnlineMetadata;
use serde_json::Value;
use std::collections::HashMap;

const API: &str = "https://api.steampowered.com";

#[derive(Clone, Debug)]
pub struct SteamWeb {
    pub key: String,
    pub steam_id: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OwnedGame {
    pub appid: u64,
    pub name: String,
    pub playtime_minutes: u64,
    pub last_played: Option<i64>,
}

impl SteamWeb {
    pub fn owned_games(&self, dl: &Downloader) -> Result<Vec<OwnedGame>, FetchError> {
        let url = format!(
            "{API}/IPlayerService/GetOwnedGames/v1/?key={}&steamid={}&include_appinfo=1&include_played_free_games=1",
            self.key, self.steam_id
        );
        Ok(parse_owned(&dl.get_json(&url)?))
    }

    /// Succès d'un jeu. `Ok(None)` : le jeu n'a pas de succès.
    pub fn achievements(&self, dl: &Downloader, appid: u64) -> Result<Option<Vec<AchievementData>>, FetchError> {
        let schema = dl.get_json(&format!(
            "{API}/ISteamUserStats/GetSchemaForGame/v2/?key={}&appid={appid}&l=french",
            self.key
        ))?;
        let defs = schema.pointer("/game/availableGameStats/achievements").and_then(Value::as_array);
        if defs.is_none_or(|d| d.is_empty()) {
            return Ok(None);
        }
        let player = dl.get_json(&format!(
            "{API}/ISteamUserStats/GetPlayerAchievements/v1/?key={}&steamid={}&appid={appid}",
            self.key, self.steam_id
        ))?;
        if player.pointer("/playerstats/success").and_then(Value::as_bool) == Some(false) {
            let msg = player.pointer("/playerstats/error").and_then(Value::as_str).unwrap_or("erreur Steam");
            return Err(FetchError::Unavailable(msg.to_string()));
        }
        // La rareté est publique ; son absence ne bloque pas la synchronisation.
        let global = dl
            .get_json(&format!("{API}/ISteamUserStats/GetGlobalAchievementPercentagesForApp/v2/?gameid={appid}"))
            .unwrap_or(Value::Null);
        Ok(Some(combine(&schema, &player, &global)))
    }
}

pub fn parse_owned(v: &Value) -> Vec<OwnedGame> {
    v.pointer("/response/games")
        .and_then(Value::as_array)
        .map(|games| {
            games
                .iter()
                .filter_map(|g| {
                    Some(OwnedGame {
                        appid: g.get("appid")?.as_u64()?,
                        name: g.get("name")?.as_str()?.to_string(),
                        playtime_minutes: g.get("playtime_forever").and_then(Value::as_u64).unwrap_or(0),
                        last_played: g.get("rtime_last_played").and_then(Value::as_i64).filter(|&t| t > 0),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn as_f32(v: &Value) -> Option<f32> {
    v.as_f64().map(|f| f as f32).or_else(|| v.as_str().and_then(|s| s.parse().ok()))
}

/// Assemble schéma + progression du joueur + rareté globale.
pub fn combine(schema: &Value, player: &Value, global: &Value) -> Vec<AchievementData> {
    let unlocked: HashMap<&str, (bool, Option<i64>)> = player
        .pointer("/playerstats/achievements")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|a| {
                    let done = a.get("achieved").and_then(Value::as_u64) == Some(1);
                    let at = a.get("unlocktime").and_then(Value::as_i64).filter(|&t| t > 0);
                    Some((a.get("apiname")?.as_str()?, (done, at)))
                })
                .collect()
        })
        .unwrap_or_default();
    let rarity: HashMap<&str, f32> = global
        .pointer("/achievementpercentages/achievements")
        .and_then(Value::as_array)
        .map(|list| list.iter().filter_map(|a| Some((a.get("name")?.as_str()?, as_f32(a.get("percent")?)?))).collect())
        .unwrap_or_default();

    schema
        .pointer("/game/availableGameStats/achievements")
        .and_then(Value::as_array)
        .map(|defs| {
            defs.iter()
                .enumerate()
                .filter_map(|(i, d)| {
                    let api = d.get("name")?.as_str()?;
                    let (done, at) = unlocked.get(api).copied().unwrap_or((false, None));
                    let s = |k: &str| d.get(k).and_then(Value::as_str).map(str::to_string);
                    Some(AchievementData {
                        api_name: api.to_string(),
                        name: s("displayName").unwrap_or_else(|| api.to_string()),
                        description: s("description").unwrap_or_default(),
                        icon_url: if done { s("icon") } else { s("icongray").or_else(|| s("icon")) },
                        hidden: d.get("hidden").and_then(Value::as_u64) == Some(1),
                        points: None,
                        rarity: rarity.get(api).copied(),
                        unlocked: done,
                        unlocked_at: at,
                        unlocked_hardcore_at: None,
                        progress: None,
                        display_order: i as i64,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Métadonnées publiques de la boutique (sans clé). `Ok(None)` : jeu absent de la boutique.
pub fn store_metadata(dl: &Downloader, appid: u64) -> Result<Option<OnlineMetadata>, FetchError> {
    let v = dl.get_json(&format!("https://store.steampowered.com/api/appdetails?appids={appid}&l=french"))?;
    Ok(parse_store(&v, appid))
}

pub fn parse_store(v: &Value, appid: u64) -> Option<OnlineMetadata> {
    let entry = v.get(appid.to_string())?;
    if entry.get("success").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    let d = entry.get("data")?;
    let list = |k: &str| -> Vec<String> {
        d.get(k).and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default()
    };
    Some(OnlineMetadata {
        description: d.get("short_description").and_then(Value::as_str).map(strip_html).filter(|s| !s.is_empty()),
        developer: list("developers").first().cloned(),
        publisher: list("publishers").first().cloned(),
        release_date: d.pointer("/release_date/date").and_then(Value::as_str).map(str::to_string).filter(|s| !s.is_empty()),
        genres: d
            .get("genres")
            .and_then(Value::as_array)
            .map(|g| g.iter().filter_map(|x| x.get("description")?.as_str().map(str::to_string)).collect())
            .unwrap_or_default(),
        screenshots: d
            .get("screenshots")
            .and_then(Value::as_array)
            .map(|s| s.iter().take(8).filter_map(|x| x.get("path_thumbnail")?.as_str().map(str::to_string)).collect())
            .unwrap_or_default(),
        tags: store_tags(d),
    })
}

/// Catégories de la boutique Steam traduites en étiquettes Halcyon.
fn store_tags(d: &Value) -> Vec<String> {
    let ids: Vec<u64> = d
        .get("categories")
        .and_then(Value::as_array)
        .map(|c| c.iter().filter_map(|x| x.get("id")?.as_u64()).collect())
        .unwrap_or_default();
    let mut tags = Vec::new();
    let mut add = |cond: bool, tag: &str| {
        if cond && !tags.iter().any(|t| t == tag) {
            tags.push(tag.to_string());
        }
    };
    let any = |list: &[u64]| ids.iter().any(|i| list.contains(i));
    // 1 multijoueur, 27 multi-plateforme, 36/37 en ligne/LAN JcJ, 47/48 LAN, 49 JcJ.
    add(any(&[1, 27, 36, 37, 47, 48, 49, 9, 38, 39, 24]), "multijoueur");
    // 9 coop, 38 coop en ligne, 39/24 écran partagé.
    add(any(&[9, 38, 39, 48]), "coop");
    add(any(&[24, 37, 39]), "écran partagé");
    add(any(&[2]), "solo");
    add(any(&[28]), "manette");
    add(any(&[18]), "manette partielle");
    tags
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn combines_schema_player_and_rarity() {
        let schema = json!({"game": {"availableGameStats": {"achievements": [
            {"name": "ACH_WIN", "displayName": "Vainqueur", "description": "Gagner", "hidden": 0,
             "icon": "https://cdn/win.jpg", "icongray": "https://cdn/win_gray.jpg"},
            {"name": "ACH_SECRET", "displayName": "Secret", "hidden": 1, "icon": "https://cdn/s.jpg", "icongray": "https://cdn/s_g.jpg"}
        ]}}});
        let player = json!({"playerstats": {"success": true, "achievements": [
            {"apiname": "ACH_WIN", "achieved": 1, "unlocktime": 1_700_000_000},
            {"apiname": "ACH_SECRET", "achieved": 0, "unlocktime": 0}
        ]}});
        // L'API renvoie parfois les pourcentages sous forme de chaînes.
        let global = json!({"achievementpercentages": {"achievements": [
            {"name": "ACH_WIN", "percent": "42.5"}, {"name": "ACH_SECRET", "percent": 1.2}
        ]}});
        let list = combine(&schema, &player, &global);
        assert_eq!(list.len(), 2);
        assert!(list[0].unlocked);
        assert_eq!(list[0].unlocked_at, Some(1_700_000_000));
        assert_eq!(list[0].rarity, Some(42.5));
        assert_eq!(list[0].icon_url.as_deref(), Some("https://cdn/win.jpg"));
        assert!(list[1].hidden && !list[1].unlocked);
        assert_eq!(list[1].icon_url.as_deref(), Some("https://cdn/s_g.jpg"), "icône grisée si verrouillé");
        assert_eq!(list[1].description, "", "jamais inventée");
    }

    #[test]
    fn parses_owned_games_and_store() {
        let owned = json!({"response": {"game_count": 1, "games": [
            {"appid": 620, "name": "Portal 2", "playtime_forever": 95, "rtime_last_played": 1_700_000_000}
        ]}});
        assert_eq!(parse_owned(&owned)[0].playtime_minutes, 95);
        assert!(parse_owned(&json!({"response": {}})).is_empty());

        let store = json!({"620": {"success": true, "data": {
            "short_description": "Une suite &quot;culte&quot;.", "developers": ["Valve"], "publishers": ["Valve"],
            "release_date": {"date": "18 avr. 2011"}, "genres": [{"id": "1", "description": "Action"}],
            "screenshots": [{"path_thumbnail": "https://cdn/1.jpg", "path_full": "https://cdn/1f.jpg"}],
            "categories": [{"id": 2, "description": "Solo"}, {"id": 9, "description": "Coop"}]
        }}});
        let m = parse_store(&store, 620).unwrap();
        assert_eq!(m.description.as_deref(), Some("Une suite \"culte\"."));
        assert_eq!(m.genres, ["Action"]);
        assert_eq!(m.screenshots, ["https://cdn/1.jpg"]);
        assert_eq!(m.tags, ["multijoueur", "coop", "solo"]);
        assert!(parse_store(&json!({"620": {"success": false}}), 620).is_none());
    }
}

//! Visuels des jeux hors plateforme (standalone, ajoutés à la main) depuis SteamGridDB
//! (fonction **en ligne**, clé d'API personnelle et gratuite : steamgriddb.com › Préférences ›
//! API). La clé est transmise dans un en-tête, jamais dans l'URL.
//!
//! Le jeu est retrouvé par son nom ; une correspondance douteuse est écartée plutôt que
//! d'afficher les images d'un autre jeu.

use crate::artwork::{Downloader, FetchError};
use crate::url::encode;
use halcyon_core::util::search_key;
use serde_json::Value;

const API: &str = "https://www.steamgriddb.com/api/v2";
/// Filtres communs : images fixes, ni contenu adulte, ni humour, ni risque d'épilepsie.
const SAFE: &str = "types=static&nsfw=false&humor=false&epilepsy=false";

/// Adresses des visuels trouvés (première image de chaque type, la mieux notée).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Art {
    pub game_id: u64,
    pub cover: Option<String>,
    pub banner: Option<String>,
    pub logo: Option<String>,
}

fn get(dl: &Downloader, key: &str, path: &str) -> Result<Value, FetchError> {
    let auth = format!("Bearer {}", key.trim());
    dl.get_json_with(&format!("{API}{path}"), &[("Authorization", auth.as_str())])
}

/// Choix du jeu parmi les résultats de recherche : nom identique (sans accents ni
/// ponctuation), sinon premier résultat dont le nom contient celui recherché ou l'inverse.
pub fn pick_game(search: &Value, name: &str) -> Option<u64> {
    let wanted = search_key(name);
    if wanted.is_empty() {
        return None;
    }
    let items = search.get("data")?.as_array()?;
    let entry = |v: &Value| Some((v.get("id")?.as_u64()?, search_key(v.get("name")?.as_str()?)));
    let all: Vec<(u64, String)> = items.iter().filter_map(entry).collect();
    all.iter()
        .find(|(_, n)| *n == wanted)
        .or_else(|| all.first().filter(|(_, n)| !n.is_empty() && (n.contains(&wanted) || wanted.contains(n.as_str()))))
        .map(|(id, _)| *id)
}

/// Première image d'une liste de résultats (`grids`, `heroes`, `logos`).
pub fn first_url(list: &Value) -> Option<String> {
    list.get("data")?
        .as_array()?
        .iter()
        .filter_map(|v| v.get("url")?.as_str())
        .find(|u| u.starts_with("https://"))
        .map(str::to_string)
}

/// Recherche le jeu puis les visuels demandés. `Ok(None)` : jeu introuvable sur SteamGridDB.
pub fn find_art(dl: &Downloader, key: &str, name: &str, want: [bool; 3]) -> Result<Option<Art>, FetchError> {
    let search = get(dl, key, &format!("/search/autocomplete/{}", encode(name.trim())))?;
    let Some(id) = pick_game(&search, name) else { return Ok(None) };
    let mut art = Art { game_id: id, ..Default::default() };
    if want[0] {
        art.cover = first_url(&get(dl, key, &format!("/grids/game/{id}?dimensions=600x900,342x482,660x930&{SAFE}"))?);
    }
    if want[1] {
        art.banner = first_url(&get(dl, key, &format!("/heroes/game/{id}?{SAFE}"))?);
    }
    if want[2] {
        art.logo = first_url(&get(dl, key, &format!("/logos/game/{id}?{SAFE}"))?);
    }
    Ok(Some(art))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn picks_exact_name_then_close_match_never_unrelated() {
        let r = json!({"success": true, "data": [
            {"id": 1, "name": "Hollow Knight: Silksong"},
            {"id": 2, "name": "Hollow Knight"}
        ]});
        assert_eq!(pick_game(&r, "hollow knight"), Some(2), "nom identique prioritaire");
        assert_eq!(pick_game(&r, "Hollow Knight : Silksong"), Some(1));
        let r = json!({"data": [{"id": 7, "name": "Celeste Classic"}]});
        assert_eq!(pick_game(&r, "Céleste"), Some(7), "nom contenu");
        let r = json!({"data": [{"id": 9, "name": "Minecraft"}]});
        assert_eq!(pick_game(&r, "Terraria"), None, "jamais les images d'un autre jeu");
        assert_eq!(pick_game(&json!({"data": []}), "Terraria"), None);
        assert_eq!(pick_game(&json!({"success": false}), "Terraria"), None);
    }

    #[test]
    fn first_https_url() {
        let r = json!({"data": [{"url": "http://x/a.png"}, {"url": "https://cdn2.steamgriddb.com/grid/a.png"}]});
        assert_eq!(first_url(&r).as_deref(), Some("https://cdn2.steamgriddb.com/grid/a.png"));
        assert_eq!(first_url(&json!({"data": []})), None);
    }
}

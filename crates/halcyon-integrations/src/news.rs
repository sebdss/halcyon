//! Actualités des jeux (Game Hub, 1.0.0) : API publique officielle de Steam
//! `ISteamNews/GetNewsForApp` (aucune clé ni connexion). Fonction **en ligne** : rien n'est
//! demandé hors connexion ou en mode hors ligne forcé. Le contenu (souvent du BBCode ou du
//! HTML) est réduit à un court extrait en texte brut ; l'article complet s'ouvre dans le
//! navigateur.

use crate::artwork::{Downloader, FetchError};

#[derive(Clone, Debug, PartialEq)]
pub struct NewsItem {
    pub title: String,
    pub url: String,
    pub source: String,
    pub date: i64,
    pub excerpt: String,
}

pub fn steam_news_url(appid: &str, count: usize) -> String {
    format!("https://api.steampowered.com/ISteamNews/GetNewsForApp/v2/?appid={appid}&count={count}&maxlength=400&format=json")
}

/// Dernières actualités d'un jeu Steam.
pub fn fetch_steam(dl: &Downloader, appid: &str, count: usize) -> Result<Vec<NewsItem>, FetchError> {
    if appid.is_empty() || !appid.chars().all(|c| c.is_ascii_digit()) {
        return Ok(Vec::new());
    }
    Ok(parse_steam(&dl.get_json(&steam_news_url(appid, count))?))
}

pub fn parse_steam(v: &serde_json::Value) -> Vec<NewsItem> {
    v.pointer("/appnews/newsitems")
        .and_then(|x| x.as_array())
        .into_iter()
        .flatten()
        .filter_map(|n| {
            let s = |k: &str| n.get(k).and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
            let url = s("url");
            // Liens limités au web (pas de schéma exotique ouvert par le navigateur).
            if !url.starts_with("https://") {
                return None;
            }
            Some(NewsItem { title: s("title"), url, source: s("feedlabel"), date: n.get("date").and_then(|d| d.as_i64()).unwrap_or(0), excerpt: plain_excerpt(&s("contents"), 220) })
        })
        .filter(|n| !n.title.is_empty())
        .collect()
}

/// Texte brut court : balises HTML et BBCode retirées, espaces condensés.
pub fn plain_excerpt(raw: &str, max: usize) -> String {
    let mut out = String::new();
    let mut skip: Option<char> = None;
    for c in raw.chars() {
        match (skip, c) {
            (None, '<') => skip = Some('>'),
            (None, '[') => skip = Some(']'),
            (Some(end), c) if c == end => {
                skip = None;
                out.push(' ');
            }
            (Some(_), _) => {}
            (None, c) => out.push(c),
        }
    }
    let text = halcyon_core::util::strip_html(&out);
    let words: Vec<&str> = text.split_whitespace().filter(|w| !w.starts_with("{STEAM_CLAN_IMAGE}") && !w.starts_with("http")).collect();
    let mut s = String::new();
    for w in words {
        if s.chars().count() + w.chars().count() + 1 > max {
            s.push('…');
            break;
        }
        if !s.is_empty() {
            s.push(' ');
        }
        s.push_str(w);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_steam_news() {
        let v = serde_json::json!({"appnews": {"appid": 620, "newsitems": [
            {"gid": "1", "title": "Patch 1.2 is live", "url": "https://store.steampowered.com/news/app/620/view/1", "feedlabel": "Community Announcements", "date": 1763288400,
             "contents": "[h1]Big update[/h1] We fixed <b>many</b> bugs. {STEAM_CLAN_IMAGE}/abc.png https://example.com more text"},
            {"gid": "2", "title": "Bad link", "url": "javascript:alert(1)", "date": 1, "contents": ""},
            {"gid": "3", "title": "", "url": "https://x", "date": 1, "contents": ""}
        ]}});
        let n = parse_steam(&v);
        assert_eq!(n.len(), 1);
        assert_eq!(n[0].title, "Patch 1.2 is live");
        assert_eq!(n[0].source, "Community Announcements");
        assert_eq!(n[0].date, 1763288400);
        assert_eq!(n[0].excerpt, "Big update We fixed many bugs. more text");
        assert!(parse_steam(&serde_json::json!({})).is_empty());
    }

    #[test]
    fn excerpt_is_bounded() {
        let long = "mot ".repeat(200);
        let e = plain_excerpt(&long, 50);
        assert!(e.chars().count() <= 51 && e.ends_with('…'));
    }
}

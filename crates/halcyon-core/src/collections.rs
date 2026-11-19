//! Collections manuelles : suggestions de séries, ordre des jeux, couleurs (logique pure).
//!
//! Une collection manuelle peut mélanger toutes les sources (Steam, GOG, ROM, jeux
//! autonomes…). Rien ici n'écrit en base : le stockage et l'interface appliquent ces
//! décisions, toujours après un accord explicite de l'utilisateur (une suggestion n'est
//! jamais créée toute seule).

use crate::model::GameId;
use crate::util::search_key;
use std::collections::{BTreeMap, HashSet};

/// Série détectée : titres qui commencent par les mêmes mots.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeriesSuggestion {
    /// Clé stable (mots normalisés), mémorisée quand la suggestion est refusée.
    pub key: String,
    /// Nom proposé, avec la casse du titre d'origine (« Resident Evil »).
    pub name: String,
    pub games: Vec<GameId>,
}

/// Mots trop génériques pour former une série à eux seuls (« The », « Dark », « Super »…).
/// Ils restent admis comme premier mot d'un préfixe plus long (« Dark Souls », « Super Mario »).
const GENERIC: &[&str] = &[
    "the", "a", "an", "of", "and", "my", "le", "la", "les", "l", "un", "une", "des", "de", "du", "el", "los", "las", "il",
    "lo", "der", "die", "das", "new", "super", "dark", "final", "little", "last", "red", "black", "white", "great", "big",
    "lost", "world", "game", "games", "edition", "pro", "free", "space", "star", "total", "grand", "real", "true", "mega",
    "ultra", "deluxe", "classic", "legend", "legends", "tales", "age", "call", "day", "night", "death", "dead", "battle",
    "war", "city", "house", "project", "ultimate",
];

/// Suffixes d'édition retirés avant comparaison (« Remastered », « GOTY »…) : deux éditions
/// d'un même jeu ne font pas une série.
const EDITION_WORDS: &[&str] = &["remastered", "remaster", "remake", "definitive", "goty", "complete", "enhanced", "hd", "edition", "redux"];

/// Mots normalisés d'un titre (sans ponctuation, sans accents) et leur forme d'origine.
fn words(title: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for raw in title.split(|c: char| c.is_whitespace() || matches!(c, ':' | '-' | '–' | '—' | '|' | '/' | '(' | ')' | '[' | ']')) {
        let norm = search_key(raw).replace(' ', "");
        if norm.is_empty() {
            continue;
        }
        let shown: String = raw.trim_matches(|c: char| !c.is_alphanumeric()).chars().filter(|c| !matches!(c, '™' | '®' | '©')).collect();
        out.push((norm, shown));
    }
    out
}

fn is_number(w: &str) -> bool {
    w.chars().all(|c| c.is_ascii_digit()) || is_roman(w)
}

fn is_roman(w: &str) -> bool {
    matches!(w, "ii" | "iii" | "iv" | "v" | "vi" | "vii" | "viii" | "ix" | "x" | "xi" | "xii" | "xiii" | "xiv" | "xv" | "xvi")
}

/// Mots communs (normalisés, d'origine), jeux et titres distincts d'un groupe.
type Group = (Vec<(String, String)>, Vec<GameId>, HashSet<String>);

fn join(w: &[(String, String)], original: bool) -> String {
    w.iter().map(|(n, s)| if original { s.as_str() } else { n.as_str() }).collect::<Vec<_>>().join(" ")
}

/// Séries détectées dans la bibliothèque. Les titres sont regroupés par leurs premiers mots,
/// jusqu'au premier mot significatif (« Resident », « Final Fantasy », « Call of Duty ») ;
/// le nom proposé est le plus long début commun à tout le groupe (« Resident Evil 2 »,
/// « Resident Evil Village » → « Resident Evil »). Il faut au moins deux titres distincts
/// (deux éditions d'un même jeu ne font pas une série).
///
/// Sont écartées les séries déjà présentes comme collection (même nom), celles refusées
/// (`dismissed`, clés) et celles dont tous les jeux sont déjà dans une collection
/// (`already_grouped`). Résultat trié par nombre de jeux décroissant, puis par nom.
pub fn suggest_series(
    games: &[(GameId, &str)],
    existing_names: &[&str],
    dismissed: &[String],
    already_grouped: &HashSet<GameId>,
) -> Vec<SeriesSuggestion> {
    // Clé de groupe → (mots du premier titre, jeux, titres distincts).
    let mut groups: BTreeMap<String, Group> = BTreeMap::new();
    for (id, title) in games {
        let mut w = words(title);
        while w.len() > 1 && w.last().is_some_and(|(n, _)| EDITION_WORDS.contains(&n.as_str())) {
            w.pop();
        }
        // Mots jusqu'au premier mot significatif (inclus).
        let Some(first) = w.iter().position(|(n, _)| !GENERIC.contains(&n.as_str()) && !is_number(n)) else { continue };
        let key_words = &w[..=first];
        let letters: usize = key_words.iter().map(|(n, _)| n.chars().filter(|c| c.is_alphabetic()).count()).sum();
        if letters < 4 {
            continue;
        }
        let key = join(key_words, false);
        let full = join(&w, false);
        let e = groups.entry(key).or_insert_with(|| (w.clone(), Vec::new(), HashSet::new()));
        // Nom : plus long début commun (mots normalisés) avec les titres déjà vus.
        let common = e.0.iter().zip(&w).take_while(|(a, b)| a.0 == b.0).count();
        e.0.truncate(common);
        e.1.push(*id);
        e.2.insert(full);
    }

    let existing: HashSet<String> = existing_names.iter().map(|n| search_key(n)).collect();
    let mut out: Vec<SeriesSuggestion> = Vec::new();
    for (key, (mut name_words, mut ids, titles)) in groups {
        if titles.len() < 2 || dismissed.contains(&key) {
            continue;
        }
        // « Halo 2 » commun à « Halo 2 » et « Halo 2 Anniversary » : le numéro n'est pas la série.
        while name_words.len() > 1 && name_words.last().is_some_and(|(n, _)| is_number(n)) {
            name_words.pop();
        }
        let name = join(&name_words, true);
        if name.is_empty() || existing.contains(&search_key(&name)) || ids.iter().all(|id| already_grouped.contains(id)) {
            continue;
        }
        ids.sort_unstable();
        ids.dedup();
        out.push(SeriesSuggestion { key, name, games: ids });
    }
    out.sort_by(|a, b| b.games.len().cmp(&a.games.len()).then_with(|| a.name.cmp(&b.name)));
    out
}

/// Ordre d'affichage des jeux d'une collection : ordre manuel (`position` > 0) d'abord,
/// puis les jeux jamais déplacés (`position` = 0), par ordre alphabétique (`sort_key`).
pub fn ordered_members(members: &[(GameId, i64)], sort_key: impl Fn(GameId) -> String) -> Vec<GameId> {
    let mut v: Vec<(i64, String, GameId)> = members.iter().map(|&(id, pos)| (if pos > 0 { pos } else { i64::MAX }, sort_key(id), id)).collect();
    v.sort();
    v.into_iter().map(|(_, _, id)| id).collect()
}

/// Déplace `item` de `delta` places dans `list` (bornes respectées). Renvoie la nouvelle
/// position, ou `None` si l'élément est absent ou ne bouge pas.
pub fn move_item<T: PartialEq>(list: &mut Vec<T>, item: &T, delta: i32) -> Option<usize> {
    let from = list.iter().position(|x| x == item)?;
    let to = (from as i64 + i64::from(delta)).clamp(0, list.len() as i64 - 1) as usize;
    if to == from {
        return None;
    }
    let x = list.remove(from);
    list.insert(to, x);
    Some(to)
}

/// Couleurs proposées pour une collection (plaque de l'étagère, pastille de l'index).
/// Teintes moyennes : texte blanc lisible dessus (contraste ≥ 4,5:1).
pub const PALETTE: [&str; 10] = ["#3d7fc4", "#c25a1c", "#2f8a5f", "#8a4fbf", "#b8336a", "#5c6b7a", "#a07818", "#1f7f8c", "#c0392b", "#4a5bc4"];

/// Couleur `#rrggbb` (ou `rrggbb`) → composantes ; `None` si invalide.
pub fn parse_color(s: &str) -> Option<(u8, u8, u8)> {
    let h = s.trim().trim_start_matches('#');
    if h.len() != 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok()?;
    Some(((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

/// Couleur par défaut d'une collection sans couleur choisie : stable d'après son nom.
pub fn default_color(name: &str) -> &'static str {
    PALETTE[(crate::util::stable_hash(&search_key(name)) % PALETTE.len() as u64) as usize]
}

/// Collections synchronisées depuis Steam : leur contenu suit Steam (non modifiable ici).
pub const STEAM_PREFIX: &str = "Steam · ";

pub fn is_synced(name: &str) -> bool {
    name.starts_with(STEAM_PREFIX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lib(titles: &[&str]) -> Vec<(GameId, String)> {
        titles.iter().enumerate().map(|(i, t)| (i as GameId + 1, t.to_string())).collect()
    }

    fn suggest(titles: &[&str], existing: &[&str], dismissed: &[&str]) -> Vec<(String, Vec<GameId>)> {
        let l = lib(titles);
        let refs: Vec<(GameId, &str)> = l.iter().map(|(i, t)| (*i, t.as_str())).collect();
        let d: Vec<String> = dismissed.iter().map(|s| s.to_string()).collect();
        suggest_series(&refs, existing, &d, &HashSet::new()).into_iter().map(|s| (s.name, s.games)).collect()
    }

    #[test]
    fn detects_numbered_and_subtitled_series() {
        let s = suggest(&["Resident Evil 2", "Resident Evil 3", "Resident Evil 4", "Resident Evil Village", "Celeste", "Hades"], &[], &[]);
        assert_eq!(s, vec![("Resident Evil".to_string(), vec![1, 2, 3, 4])]);
    }

    #[test]
    fn single_word_series_and_trademarks() {
        let s = suggest(&["Sonic Mania", "Sonic Frontiers", "Sonic Generations™", "Celeste"], &[], &[]);
        assert_eq!(s, vec![("Sonic".to_string(), vec![1, 2, 3])]);
    }

    #[test]
    fn generic_words_and_editions_are_not_series() {
        // « The » et « Dark » seuls ne sont pas des séries ; deux éditions d'un jeu non plus.
        assert!(suggest(&["The Witcher 3", "The Last of Us", "Dark Messiah", "Darkest Dungeon"], &[], &[]).is_empty());
        assert!(suggest(&["Dark Souls Remastered", "Dark Souls"], &[], &[]).is_empty());
        // Mais « Dark Souls » + « Dark Souls III » en est une.
        let s = suggest(&["Dark Souls Remastered", "Dark Souls II", "Dark Souls III"], &[], &[]);
        assert_eq!(s, vec![("Dark Souls".to_string(), vec![1, 2, 3])]);
    }

    #[test]
    fn longest_shared_prefix_wins() {
        let s = suggest(&["Final Fantasy VII", "Final Fantasy X", "Final Fantasy XIV Online", "Final Fight"], &[], &[]);
        assert_eq!(s, vec![("Final Fantasy".to_string(), vec![1, 2, 3])]);
    }

    #[test]
    fn dismissed_and_existing_are_skipped() {
        let titles = ["Sonic Mania", "Sonic Frontiers", "Halo 2", "Halo 3", "Halo Infinite"];
        assert_eq!(suggest(&titles, &[], &[]).len(), 2);
        let s = suggest(&titles, &["sonic"], &["halo"]);
        assert!(s.is_empty(), "{s:?}");
        let l = lib(&titles);
        let refs: Vec<(GameId, &str)> = l.iter().map(|(i, t)| (*i, t.as_str())).collect();
        let grouped: HashSet<GameId> = [1, 2].into_iter().collect();
        let s = suggest_series(&refs, &[], &[], &grouped);
        assert_eq!(s.iter().map(|x| x.name.as_str()).collect::<Vec<_>>(), vec!["Halo"]);
        assert_eq!(s[0].key, "halo");
    }

    #[test]
    fn sub_series_join_the_main_series() {
        let s = suggest(&["Sonic Mania", "Sonic Adventure", "Sonic Adventure 2", "Call of Duty 4", "Call of Duty: Black Ops"], &[], &[]);
        assert_eq!(s, vec![("Sonic".to_string(), vec![1, 2, 3]), ("Call of Duty".to_string(), vec![4, 5])]);
    }

    #[test]
    fn manual_order_then_alphabetical() {
        use std::collections::HashMap;
        let names: HashMap<GameId, &str> = [(1, "Zelda"), (2, "Mario"), (3, "Celeste"), (4, "Astro")].into_iter().collect();
        let key = |id: GameId| names[&id].to_lowercase();
        assert_eq!(ordered_members(&[(1, 0), (2, 0), (3, 0)], key), vec![3, 2, 1]);
        assert_eq!(ordered_members(&[(1, 1), (2, 2), (3, 0), (4, 0)], key), vec![1, 2, 4, 3]);
    }

    #[test]
    fn moving_items_respects_bounds() {
        let mut v = vec![1, 2, 3, 4];
        assert_eq!(move_item(&mut v, &3, -1), Some(1));
        assert_eq!(v, vec![1, 3, 2, 4]);
        assert_eq!(move_item(&mut v, &1, -1), None);
        assert_eq!(move_item(&mut v, &1, 10), Some(3));
        assert_eq!(v, vec![3, 2, 4, 1]);
        assert_eq!(move_item(&mut v, &9, 1), None);
    }

    #[test]
    fn colors() {
        assert_eq!(parse_color("#3d9be9"), Some((0x3d, 0x9b, 0xe9)));
        assert_eq!(parse_color("3D9BE9"), Some((0x3d, 0x9b, 0xe9)));
        assert_eq!(parse_color("#fff"), None);
        assert_eq!(parse_color("#zzzzzz"), None);
        assert_eq!(default_color("Sonic"), default_color("sonic"));
        assert!(PALETTE.iter().all(|c| parse_color(c).is_some()));
        assert!(is_synced("Steam · Favoris") && !is_synced("Sonic"));
    }
}

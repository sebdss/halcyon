//! Recherche, filtres et tri de la bibliothèque — logique pure, exécutée en mémoire.
//!
//! La bibliothèque complète tient en mémoire (quelques Mo pour 10 000 jeux) ;
//! filtrer ici est plus rapide qu'un aller-retour SQL à chaque frappe.

use crate::model::{Game, GameId, GameSource};
use crate::search::{MatchTier, QueryContext, SearchQuery};
use std::cmp::Ordering;
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SourceFilter {
    #[default]
    All,
    Only(GameSource),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SortKey {
    #[default]
    Name,
    LastPlayed,
    RecentlyAdded,
    Playtime,
}

impl SortKey {
    pub const ALL: [SortKey; 4] = [Self::Name, Self::LastPlayed, Self::RecentlyAdded, Self::Playtime];

    pub fn label(self) -> String {
        match self {
            Self::Name => tr!("Nom"),
            Self::LastPlayed => "Dernier lancement".into(),
            Self::RecentlyAdded => tr!("Ajout récent"),
            Self::Playtime => tr!("Temps de jeu"),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::LastPlayed => "last_played",
            Self::RecentlyAdded => "recently_added",
            Self::Playtime => "playtime",
        }
    }

    pub fn parse(s: &str) -> Self {
        Self::ALL.into_iter().find(|k| k.as_str() == s).unwrap_or_default()
    }
}

/// Critères d'affichage de la bibliothèque.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LibraryQuery {
    pub text: String,
    pub source: SourceFilter,
    pub installed_only: bool,
    pub favorites_only: bool,
    pub include_hidden: bool,
    pub sort: SortKey,
    /// Genre exact (insensible à la casse).
    pub genre: Option<String>,
    /// Membres de la collection sélectionnée.
    pub collection: Option<HashSet<GameId>>,
}

impl LibraryQuery {
    /// Retourne les indices (dans `games`) des jeux correspondants, triés.
    pub fn apply(&self, games: &[Game]) -> Vec<usize> {
        self.apply_with(games, &QueryContext::default())
    }

    /// Comme [`apply`](Self::apply), avec les données nécessaires aux filtres avancés
    /// (succès, noms de plateformes). Les correspondances approximatives passent après
    /// les correspondances exactes, chaque groupe gardant le tri choisi.
    pub fn apply_with(&self, games: &[Game], ctx: &QueryContext) -> Vec<usize> {
        let search = SearchQuery::parse(&self.text);
        let mut fuzzy: Vec<usize> = Vec::new();
        let mut out: Vec<usize> = Vec::new();
        for (i, g) in games.iter().enumerate() {
            if !self.matches(g) {
                continue;
            }
            match search.matches(g, ctx) {
                Some(MatchTier::Direct) => out.push(i),
                Some(MatchTier::Fuzzy) => fuzzy.push(i),
                None => {}
            }
        }
        self.sort(games, &mut out);
        self.sort(games, &mut fuzzy);
        out.extend(fuzzy);
        out
    }

    fn sort(&self, games: &[Game], out: &mut [usize]) {
        let by_name = |a: &Game, b: &Game| a.sort_key.cmp(&b.sort_key);
        match self.sort {
            SortKey::Name => out.sort_unstable_by(|&a, &b| by_name(&games[a], &games[b])),
            SortKey::LastPlayed => out.sort_unstable_by(|&a, &b| {
                desc_opt(games[a].last_played, games[b].last_played).then_with(|| by_name(&games[a], &games[b]))
            }),
            SortKey::RecentlyAdded => out.sort_unstable_by(|&a, &b| {
                games[b].added_at.cmp(&games[a].added_at).then_with(|| by_name(&games[a], &games[b]))
            }),
            SortKey::Playtime => out.sort_unstable_by(|&a, &b| {
                games[b]
                    .effective_playtime()
                    .cmp(&games[a].effective_playtime())
                    .then_with(|| by_name(&games[a], &games[b]))
            }),
        }
    }

    fn matches(&self, g: &Game) -> bool {
        if g.hidden && !self.include_hidden {
            return false;
        }
        if let SourceFilter::Only(s) = self.source {
            if g.source != s && !(s == GameSource::Standalone && g.source == GameSource::Manual) {
                return false;
            }
        }
        if self.installed_only && !g.installed {
            return false;
        }
        if self.favorites_only && !g.favorite {
            return false;
        }
        if let Some(genre) = &self.genre {
            if !g.genres.iter().any(|x| x.eq_ignore_ascii_case(genre)) {
                return false;
            }
        }
        if let Some(members) = &self.collection {
            if !members.contains(&g.id) {
                return false;
            }
        }
        true
    }
}

/// Tri décroissant où `None` (jamais lancé) passe en dernier.
fn desc_opt(a: Option<i64>, b: Option<i64>) -> Ordering {
    match (a, b) {
        (Some(x), Some(y)) => y.cmp(&x),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn game(id: i64, name: &str, source: GameSource) -> Game {
        Game::blank(id, name, source)
    }

    fn names(games: &[Game], idx: &[usize]) -> Vec<String> {
        idx.iter().map(|&i| games[i].name.clone()).collect()
    }

    #[test]
    fn search_is_accent_and_order_insensitive() {
        let games = vec![
            game(1, "Pokémon Écarlate", GameSource::Manual),
            game(2, "Hollow Knight", GameSource::Steam),
            game(3, "Knights of Pen & Paper", GameSource::Steam),
        ];
        let q = LibraryQuery { text: "ecarl poke".into(), ..Default::default() };
        assert_eq!(names(&games, &q.apply(&games)), ["Pokémon Écarlate"]);
        let q = LibraryQuery { text: "KNIGHT".into(), ..Default::default() };
        assert_eq!(names(&games, &q.apply(&games)), ["Hollow Knight", "Knights of Pen & Paper"]);
    }

    #[test]
    fn filters_and_sorts() {
        let mut games = vec![
            game(1, "Celeste", GameSource::Epic),
            game(2, "Hades", GameSource::Steam),
            game(3, "Doom", GameSource::Steam),
            game(4, "Secret", GameSource::Steam),
        ];
        games[1].last_played = Some(100);
        games[2].last_played = Some(200);
        games[1].favorite = true;
        games[0].installed = false;
        games[3].hidden = true;

        let q = LibraryQuery { source: SourceFilter::Only(GameSource::Steam), ..Default::default() };
        assert_eq!(names(&games, &q.apply(&games)), ["Doom", "Hades"]);

        let q = LibraryQuery { sort: SortKey::LastPlayed, ..Default::default() };
        assert_eq!(names(&games, &q.apply(&games)), ["Doom", "Hades", "Celeste"]);

        let q = LibraryQuery { installed_only: true, favorites_only: true, ..Default::default() };
        assert_eq!(names(&games, &q.apply(&games)), ["Hades"]);

        let q = LibraryQuery { include_hidden: true, ..Default::default() };
        assert_eq!(q.apply(&games).len(), 4);

        games[2].genres = vec!["Action".into()];
        let q = LibraryQuery { genre: Some("action".into()), ..Default::default() };
        assert_eq!(names(&games, &q.apply(&games)), ["Doom"]);
        let q = LibraryQuery { collection: Some([1, 2].into_iter().collect()), ..Default::default() };
        assert_eq!(names(&games, &q.apply(&games)), ["Celeste", "Hades"]);
    }

    /// Garde-fou de performance : filtrer et trier 10 000 jeux doit rester instantané.
    #[test]
    fn ten_thousand_games_filter_fast() {
        let games: Vec<Game> =
            (0..10_000).map(|i| game(i, &format!("Game Title Number {i} Edition"), GameSource::Steam)).collect();
        let q = LibraryQuery { text: "number 9".into(), ..Default::default() };
        let start = std::time::Instant::now();
        let mut total = 0;
        for _ in 0..10 {
            total += q.apply(&games).len();
        }
        let per_query = start.elapsed() / 10;
        assert_eq!(total / 10, 3439); // nombres de 0 à 9999 contenant un 9
        // Large marge pour les machines de CI lentes et les builds debug.
        assert!(per_query.as_millis() < 50, "requête trop lente : {per_query:?}");
        eprintln!("10 000 jeux filtrés + triés en {per_query:?}");

        // Pire cas : aucun mot ne correspond directement, la recherche approximative
        // est évaluée sur chaque jeu.
        let q = LibraryQuery { text: "editoin nmber".into(), ..Default::default() };
        let start = std::time::Instant::now();
        let n = q.apply(&games).len();
        let fuzzy = start.elapsed();
        assert_eq!(n, 10_000);
        assert!(fuzzy.as_millis() < 250, "recherche approximative trop lente : {fuzzy:?}");
        eprintln!("10 000 jeux en recherche approximative : {fuzzy:?}");
    }
}

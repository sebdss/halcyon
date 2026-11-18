//! Assistant local « Que jouer ? » : suggestions calculées uniquement à partir des
//! données de la bibliothèque (temps de jeu, sessions, statut, succès, genres).
//! Aucun service distant n'est utilisé ; chaque suggestion est justifiée.

use crate::achievements::Progress;
use crate::model::{Game, GameId, PlayStatus};
use std::collections::HashMap;

const DAY: i64 = 86_400;

/// Genres qui se prêtent souvent aux sessions courtes (quand aucune session n'est connue).
const SHORT_GENRES: &[&str] = &["puzzle", "arcade", "casual", "roguelike", "rogue-lite", "roguelite", "card", "cartes", "racing", "course", "fighting", "combat", "rhythm"];

#[derive(Clone, Debug, Default)]
pub struct Context {
    pub now: i64,
    /// Durées des sessions suivies, par jeu.
    pub sessions: HashMap<GameId, Vec<u64>>,
    pub progress: HashMap<GameId, Progress>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Suggestion {
    pub game: GameId,
    pub reason: String,
    pub score: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Suggestions {
    /// Adaptés au temps disponible.
    pub for_time: Vec<Suggestion>,
    pub resume: Vec<Suggestion>,
    pub almost_done: Vec<Suggestion>,
    pub discover: Vec<Suggestion>,
    pub rediscover: Vec<Suggestion>,
    /// Genres préférés (pondérés par le temps de jeu), pour l'affichage.
    pub top_genres: Vec<String>,
}

fn genre_key(g: &str) -> String {
    crate::util::search_key(g)
}

/// Genres préférés : temps de jeu cumulé par genre.
pub fn top_genres(games: &[&Game], n: usize) -> Vec<(String, u64)> {
    let mut by: HashMap<String, (String, u64)> = HashMap::new();
    for g in games {
        let t = g.effective_playtime();
        if t == 0 {
            continue;
        }
        for genre in &g.genres {
            let e = by.entry(genre_key(genre)).or_insert_with(|| (genre.clone(), 0));
            e.1 += t;
        }
    }
    let mut v: Vec<(String, u64)> = by.into_values().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    v.truncate(n);
    v
}

/// `minutes` : temps disponible (0 = sans limite).
pub fn suggest(games: &[Game], ctx: &Context, minutes: u32, per_section: usize) -> Suggestions {
    let pool: Vec<&Game> = games.iter().filter(|g| g.installed && !g.hidden).collect();
    let genres = top_genres(&pool, 5);
    let fav_genres: Vec<String> = genres.iter().map(|(g, _)| genre_key(g)).collect();
    let genre_match = |g: &Game| g.genres.iter().map(|x| genre_key(x)).find(|k| fav_genres.contains(k));
    let avg = |id: GameId| -> Option<u64> {
        ctx.sessions.get(&id).filter(|s| !s.is_empty()).map(|s| s.iter().sum::<u64>() / s.len() as u64)
    };
    let done = |g: &Game| matches!(g.status, PlayStatus::Completed | PlayStatus::Abandoned);
    let mut out = Suggestions { top_genres: genres.iter().map(|(g, _)| g.clone()).collect(), ..Default::default() };

    // Temps disponible : durée moyenne des sessions connues, sinon genre « court ».
    if minutes > 0 {
        let budget = minutes as u64 * 60;
        let mut v: Vec<Suggestion> = pool
            .iter()
            .filter(|g| !done(g))
            .filter_map(|g| {
                if let Some(a) = avg(g.id) {
                    if a <= budget && a >= 5 * 60 {
                        let fit = 1.0 - (budget - a) as f32 / budget as f32;
                        return Some(Suggestion {
                            game: g.id,
                            reason: tr!("Vos sessions durent en moyenne {a}", a = crate::util::format_playtime(a)),
                            score: 2.0 + fit + recency_bonus(g, ctx.now),
                        });
                    }
                    return None;
                }
                let short = g.genres.iter().any(|x| SHORT_GENRES.iter().any(|s| genre_key(x).contains(s)));
                (minutes <= 45 && short).then(|| Suggestion {
                    game: g.id,
                    reason: tr!("Genre adapté aux parties courtes").into(),
                    score: 1.0 + recency_bonus(g, ctx.now),
                })
            })
            .collect();
        sort_take(&mut v, per_section);
        out.for_time = v;
    }

    // À reprendre : statut « en cours », ou joué ces 30 derniers jours et non terminé.
    let mut v: Vec<Suggestion> = pool
        .iter()
        .filter(|g| !done(g))
        .filter_map(|g| {
            let recent = g.last_played.is_some_and(|t| ctx.now - t <= 30 * DAY);
            if g.status == PlayStatus::Playing {
                Some(Suggestion { game: g.id, reason: tr!("Marqué « en cours »").into(), score: 3.0 + recency_bonus(g, ctx.now) })
            } else if recent && g.effective_playtime() > 0 {
                let days = (ctx.now - g.last_played.unwrap_or(ctx.now)) / DAY;
                Some(Suggestion {
                    game: g.id,
                    reason: match days {
                        0 => tr!("Joué aujourd'hui").into(),
                        1 => tr!("Joué hier").into(),
                        d => trn!(d, "Joué il y a {n} jour", "Joué il y a {n} jours"),
                    },
                    score: 1.0 + recency_bonus(g, ctx.now),
                })
            } else {
                None
            }
        })
        .collect();
    sort_take(&mut v, per_section);
    out.resume = v;

    // Presque terminé : progression des succès ≥ 70 %.
    let mut v: Vec<Suggestion> = pool
        .iter()
        .filter_map(|g| {
            let p = ctx.progress.get(&g.id).filter(|p| p.total > 0 && !p.complete())?;
            let pct = p.percent();
            (pct >= 70.0).then(|| Suggestion {
                game: g.id,
                reason: trn!(p.total - p.unlocked, "{unlocked} / {total} succès ({pct} %) : il en reste {n}", "{unlocked} / {total} succès ({pct} %) : il en reste {n}", unlocked = p.unlocked, total = p.total, pct = format!("{:.0}", pct)),
                score: pct / 100.0 + 1.0,
            })
        })
        .collect();
    sort_take(&mut v, per_section);
    out.almost_done = v;

    // À découvrir : jamais lancés, de préférence dans un genre apprécié.
    let mut v: Vec<Suggestion> = pool
        .iter()
        .filter(|g| g.last_played.is_none() && g.effective_playtime() == 0 && !done(g))
        .map(|g| {
            let added_days = (ctx.now - g.added_at).max(0) / DAY;
            match genre_match(g) {
                Some(_) => Suggestion {
                    game: g.id,
                    reason: tr!("Jamais lancé · genre que vous appréciez ({v})", v = g.genres.first().cloned().unwrap_or_default()),
                    score: 2.0 - (added_days as f32 / 3650.0).min(0.5),
                },
                None => Suggestion {
                    game: g.id,
                    reason: if g.status == PlayStatus::Backlog { tr!("Dans votre liste « à jouer »").into() } else { tr!("Jamais lancé").into() },
                    score: if g.status == PlayStatus::Backlog { 1.8 } else { 1.0 } - (added_days as f32 / 3650.0).min(0.5),
                },
            }
        })
        .collect();
    sort_take(&mut v, per_section);
    out.discover = v;

    // Redécouvrir : beaucoup joué, pas lancé depuis plus de 3 mois.
    let mut v: Vec<Suggestion> = pool
        .iter()
        .filter(|g| g.effective_playtime() >= 5 * 3600 && g.last_played.is_some_and(|t| ctx.now - t > 90 * DAY))
        .map(|g| {
            let months = (ctx.now - g.last_played.unwrap_or(ctx.now)) / (30 * DAY);
            Suggestion {
                game: g.id,
                reason: trn!(months, "{playtime} de jeu · pas lancé depuis {n} mois", "{playtime} de jeu · pas lancé depuis {n} mois", playtime = crate::util::format_playtime(g.effective_playtime())),
                score: (g.effective_playtime() as f32 / 3600.0).ln_1p(),
            }
        })
        .collect();
    sort_take(&mut v, per_section);
    out.rediscover = v;
    out
}

fn recency_bonus(g: &Game, now: i64) -> f32 {
    match g.last_played {
        Some(t) => (1.0 - ((now - t) as f32 / (60.0 * DAY as f32))).clamp(0.0, 1.0),
        None => 0.0,
    }
}

fn sort_take(v: &mut Vec<Suggestion>, n: usize) {
    v.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal).then(a.game.cmp(&b.game)));
    v.truncate(n);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::GameSource;

    #[test]
    fn suggestions_are_justified_by_data() {
        let now = 1000 * DAY;
        let mut a = Game::blank(1, "Court", GameSource::Steam);
        a.genres = vec!["RPG".into()];
        a.playtime_secs = 40 * 3600;
        a.last_played = Some(now - 2 * DAY);
        let mut b = Game::blank(2, "Jamais lancé RPG", GameSource::Steam);
        b.genres = vec!["RPG".into()];
        let mut c = Game::blank(3, "Ancien", GameSource::Epic);
        c.playtime_secs = 30 * 3600;
        c.last_played = Some(now - 200 * DAY);
        let mut d = Game::blank(4, "Fini", GameSource::Steam);
        d.status = PlayStatus::Completed;
        d.last_played = Some(now - DAY);
        let mut e = Game::blank(5, "Puzzle", GameSource::Steam);
        e.genres = vec!["Puzzle".into()];
        let mut ctx = Context { now, ..Default::default() };
        ctx.sessions.insert(1, vec![20 * 60, 30 * 60]);
        ctx.progress.insert(3, Progress { unlocked: 9, total: 10 });

        let s = suggest(&[a, b, c, d, e], &ctx, 30, 5);
        assert_eq!(s.top_genres, ["RPG"]);
        assert_eq!(s.for_time.iter().map(|x| x.game).collect::<Vec<_>>(), [1, 5]);
        assert!(s.for_time[0].reason.contains("25 min"));
        assert_eq!(s.resume.iter().map(|x| x.game).collect::<Vec<_>>(), [1], "les jeux terminés ne sont pas proposés");
        assert_eq!(s.almost_done[0].game, 3);
        assert_eq!(s.discover[0].game, 2, "genre apprécié en premier");
        assert_eq!(s.rediscover[0].game, 3);
        let none = suggest(&[], &ctx, 30, 5);
        assert!(none.for_time.is_empty() && none.discover.is_empty());
    }
}

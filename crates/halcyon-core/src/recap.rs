//! Récapitulatif d'une semaine, d'un mois ou d'une année (logique pure, testable).
//!
//! Calculé sur les **sessions suivies par Halcyon** (début, durée, jeu) et les succès
//! débloqués : les plateformes ne donnent qu'un temps total, pas son étalement dans le temps.
//! Toutes les périodes sont en heure locale (`tz` : décalage UTC en secondes).

use crate::util::{civil_from_days, days_from_civil};
use crate::GameId;
use std::collections::{HashMap, HashSet};

const DAY: i64 = 86_400;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Period {
    Week,
    Month,
    Year,
}

impl Period {
    pub const ALL: [Period; 3] = [Period::Week, Period::Month, Period::Year];
}

/// Session de jeu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Play {
    pub game: GameId,
    pub started_at: i64,
    pub secs: u64,
}

/// Succès débloqué (rareté : % de joueurs, si connue).
#[derive(Clone, Debug, PartialEq)]
pub struct Unlock {
    pub game: GameId,
    pub name: String,
    pub at: i64,
    pub rarity: Option<f32>,
}

/// Moment de la journée.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DayPart {
    /// 5 h – 12 h
    Morning,
    /// 12 h – 18 h
    Afternoon,
    /// 18 h – 23 h
    Evening,
    /// 23 h – 5 h
    Night,
}

impl DayPart {
    fn of(hour: i64) -> Self {
        match hour {
            5..=11 => Self::Morning,
            12..=17 => Self::Afternoon,
            18..=22 => Self::Evening,
            _ => Self::Night,
        }
    }

    pub fn label(self) -> String {
        match self {
            Self::Morning => tr!("le matin"),
            Self::Afternoon => tr!("l'après-midi"),
            Self::Evening => tr!("le soir"),
            Self::Night => tr!("la nuit"),
        }
    }
}

/// Profil de joueur de la période (le premier critère rempli l'emporte).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    /// Un jeu fait au moins 60 % du temps.
    Loyal,
    /// Au moins 40 % du temps la nuit.
    NightOwl,
    /// Sessions longues (moyenne ≥ 2 h).
    Marathoner,
    /// Au moins 15 succès débloqués.
    Hunter,
    /// Au moins 6 jeux différents.
    Explorer,
    /// Joué au moins 70 % des jours de la période.
    Regular,
    /// Au moins 40 % du temps le matin.
    EarlyBird,
    /// Le reste.
    Casual,
}

impl Profile {
    pub fn title(self) -> String {
        match self {
            Self::Loyal => tr!("Le fidèle"),
            Self::NightOwl => tr!("Le noctambule"),
            Self::Marathoner => tr!("Le marathonien"),
            Self::Hunter => tr!("Le chasseur de succès"),
            Self::Explorer => tr!("L'explorateur"),
            Self::Regular => tr!("Le régulier"),
            Self::EarlyBird => tr!("Le lève-tôt"),
            Self::Casual => tr!("Le joueur du dimanche"),
        }
    }

    pub fn description(self) -> String {
        match self {
            Self::Loyal => tr!("Un jeu vous a tenu compagnie presque tout le temps."),
            Self::NightOwl => tr!("Vos meilleures parties se jouent quand tout le monde dort."),
            Self::Marathoner => tr!("Quand vous lancez un jeu, c'est pour longtemps."),
            Self::Hunter => tr!("Les succès ne vous échappent pas."),
            Self::Explorer => tr!("Vous aimez passer d'un monde à l'autre."),
            Self::Regular => tr!("Un peu de jeu presque chaque jour."),
            Self::EarlyBird => tr!("Vous jouez avant que la journée commence."),
            Self::Casual => tr!("Le jeu, quand l'envie vient : sans pression."),
        }
    }
}

/// Récapitulatif.
#[derive(Clone, Debug, PartialEq)]
pub struct Recap {
    pub period: Period,
    /// Début et fin (exclue) de la période, horodatages Unix.
    pub start: i64,
    pub end: i64,
    pub total_secs: u64,
    pub previous_secs: u64,
    pub sessions: usize,
    pub days_played: usize,
    /// Jours de la période (écoulés seulement, pour la période en cours).
    pub days_in_period: usize,
    /// Plus longue série de jours consécutifs avec au moins une session.
    pub best_streak: u32,
    /// Jeux par temps décroissant : (jeu, secondes, sessions).
    pub top: Vec<(GameId, u64, usize)>,
    /// Plus longue session : (jeu, secondes, début).
    pub longest: Option<(GameId, u64, i64)>,
    /// Jour préféré (0 = lundi).
    pub weekday: Option<u8>,
    pub day_part: Option<DayPart>,
    /// Jeux joués pour la première fois pendant la période.
    pub new_games: usize,
    pub unlocks: usize,
    pub rarest: Option<Unlock>,
    pub profile: Profile,
}

fn local_day(t: i64, tz: i64) -> i64 {
    (t + tz).div_euclid(DAY)
}

/// Bornes `[début, fin)` de la période contenant `now`, décalée de `offset` (−1 : la précédente).
pub fn bounds(period: Period, now: i64, tz: i64, offset: i64) -> (i64, i64) {
    let today = local_day(now, tz);
    let (start_day, end_day) = match period {
        Period::Week => {
            let monday = today - (today + 3).rem_euclid(7) + 7 * offset;
            (monday, monday + 7)
        }
        Period::Month => {
            let (y, m, _) = civil_from_days(today);
            let idx = y * 12 + (m as i64 - 1) + offset;
            let (ny, nm) = (idx.div_euclid(12), (idx.rem_euclid(12) + 1) as u32);
            let next = idx + 1;
            (days_from_civil(ny, nm, 1), days_from_civil(next.div_euclid(12), (next.rem_euclid(12) + 1) as u32, 1))
        }
        Period::Year => {
            let (y, _, _) = civil_from_days(today);
            (days_from_civil(y + offset, 1, 1), days_from_civil(y + offset + 1, 1, 1))
        }
    };
    (start_day * DAY - tz, end_day * DAY - tz)
}

/// Calcule le récapitulatif de la période `offset` (0 : en cours). Un jeu est « nouveau »
/// si sa toute première session tombe dans la période.
pub fn compute(period: Period, offset: i64, plays: &[Play], unlocks: &[Unlock], now: i64, tz: i64) -> Recap {
    let (start, end) = bounds(period, now, tz, offset);
    let (prev_start, _) = bounds(period, now, tz, offset - 1);
    let inside: Vec<&Play> = plays.iter().filter(|p| p.started_at >= start && p.started_at < end).collect();
    let total_secs: u64 = inside.iter().map(|p| p.secs).sum();
    let previous_secs: u64 = plays.iter().filter(|p| p.started_at >= prev_start && p.started_at < start).map(|p| p.secs).sum();

    let mut per_game: HashMap<GameId, (u64, usize)> = HashMap::new();
    let mut days: HashSet<i64> = HashSet::new();
    let mut weekdays = [0u64; 7];
    let mut parts: HashMap<DayPart, u64> = HashMap::new();
    for p in &inside {
        let e = per_game.entry(p.game).or_default();
        e.0 += p.secs;
        e.1 += 1;
        let day = local_day(p.started_at, tz);
        days.insert(day);
        weekdays[(day + 3).rem_euclid(7) as usize] += p.secs;
        let hour = (p.started_at + tz).rem_euclid(DAY) / 3600;
        *parts.entry(DayPart::of(hour)).or_default() += p.secs;
    }
    let mut top: Vec<(GameId, u64, usize)> = per_game.into_iter().map(|(g, (s, n))| (g, s, n)).collect();
    top.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    let mut sorted: Vec<i64> = days.iter().copied().collect();
    sorted.sort_unstable();
    let (mut best, mut run) = (0u32, 0u32);
    for (i, d) in sorted.iter().enumerate() {
        run = if i > 0 && d - sorted[i - 1] == 1 { run + 1 } else { 1 };
        best = best.max(run);
    }

    let longest = inside.iter().max_by_key(|p| (p.secs, std::cmp::Reverse(p.started_at))).map(|p| (p.game, p.secs, p.started_at));
    let weekday = (total_secs > 0).then(|| (0..7).max_by_key(|&d| (weekdays[d], std::cmp::Reverse(d))).map(|d| d as u8)).flatten();
    let day_part = parts.iter().max_by_key(|(k, v)| (**v, matches!(k, DayPart::Evening))).map(|(k, _)| *k);

    // Premier jeu joué de chaque jeu, toutes périodes confondues.
    let mut first: HashMap<GameId, i64> = HashMap::new();
    for p in plays {
        let e = first.entry(p.game).or_insert(p.started_at);
        *e = (*e).min(p.started_at);
    }
    let new_games = first.values().filter(|&&t| t >= start && t < end).count();

    let period_unlocks: Vec<&Unlock> = unlocks.iter().filter(|u| u.at >= start && u.at < end).collect();
    let rarest = period_unlocks
        .iter()
        .filter(|u| u.rarity.is_some())
        .min_by(|a, b| a.rarity.partial_cmp(&b.rarity).unwrap_or(std::cmp::Ordering::Equal))
        .map(|u| (*u).clone());

    let today_end = local_day(now, tz) + 1;
    let days_in_period = ((end.min(today_end * DAY - tz) - start) / DAY).max(1) as usize;

    let share = |secs: u64| if total_secs == 0 { 0.0 } else { secs as f64 / total_secs as f64 };
    let profile = if total_secs == 0 {
        Profile::Casual
    } else if top.first().is_some_and(|t| share(t.1) >= 0.6) && top.len() > 1 {
        Profile::Loyal
    } else if share(parts.get(&DayPart::Night).copied().unwrap_or(0)) >= 0.4 {
        Profile::NightOwl
    } else if !inside.is_empty() && total_secs / inside.len() as u64 >= 2 * 3600 {
        Profile::Marathoner
    } else if period_unlocks.len() >= 15 {
        Profile::Hunter
    } else if top.len() >= 6 {
        Profile::Explorer
    } else if days.len() * 10 >= days_in_period * 7 && days_in_period >= 3 {
        Profile::Regular
    } else if share(parts.get(&DayPart::Morning).copied().unwrap_or(0)) >= 0.4 {
        Profile::EarlyBird
    } else if top.len() == 1 {
        Profile::Loyal
    } else {
        Profile::Casual
    };

    Recap {
        period,
        start,
        end,
        total_secs,
        previous_secs,
        sessions: inside.len(),
        days_played: days.len(),
        days_in_period,
        best_streak: best,
        top,
        longest,
        weekday,
        day_part,
        new_games,
        unlocks: period_unlocks.len(),
        rarest,
        profile,
    }
}

/// Nom du jour de la semaine (0 = lundi).
pub fn weekday_name(d: u8) -> String {
    match d {
        0 => tr!("lundi"),
        1 => tr!("mardi"),
        2 => tr!("mercredi"),
        3 => tr!("jeudi"),
        4 => tr!("vendredi"),
        5 => tr!("samedi"),
        _ => tr!("dimanche"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mercredi 13 novembre 2024, 12:00 UTC ; fuseau UTC+1.
    const NOW: i64 = 1_731_499_200;
    const TZ: i64 = 3600;

    fn at(day: i64, hour: i64) -> i64 {
        // Jour local relatif au lundi 11 novembre 2024.
        let monday = days_from_civil(2024, 11, 11);
        (monday + day) * DAY + hour * 3600 - TZ
    }

    #[test]
    fn period_bounds() {
        let (s, e) = bounds(Period::Week, NOW, TZ, 0);
        assert_eq!((s, e), (at(0, 0), at(7, 0)));
        let (s, _) = bounds(Period::Week, NOW, TZ, -1);
        assert_eq!(s, at(-7, 0));
        let (s, e) = bounds(Period::Month, NOW, TZ, 0);
        assert_eq!((s, e), (days_from_civil(2024, 11, 1) * DAY - TZ, days_from_civil(2024, 12, 1) * DAY - TZ));
        let (s, _) = bounds(Period::Month, NOW, TZ, -11);
        assert_eq!(s, days_from_civil(2023, 12, 1) * DAY - TZ);
        let (s, e) = bounds(Period::Year, NOW, TZ, 0);
        assert_eq!((s, e), (days_from_civil(2024, 1, 1) * DAY - TZ, days_from_civil(2025, 1, 1) * DAY - TZ));
    }

    #[test]
    fn weekly_recap() {
        let plays = vec![
            Play { game: 1, started_at: at(0, 21), secs: 3 * 3600 },
            Play { game: 1, started_at: at(1, 22), secs: 2 * 3600 },
            Play { game: 2, started_at: at(2, 9), secs: 1800 },
            // Semaine précédente, et jeu 3 découvert avant.
            Play { game: 3, started_at: at(-3, 20), secs: 3600 },
            Play { game: 1, started_at: at(-14, 20), secs: 600 },
        ];
        let unlocks = vec![
            Unlock { game: 1, name: "Rare".into(), at: at(1, 23), rarity: Some(2.5) },
            Unlock { game: 1, name: "Commun".into(), at: at(1, 23), rarity: Some(60.0) },
            Unlock { game: 2, name: "Avant".into(), at: at(-2, 10), rarity: Some(0.1) },
        ];
        let r = compute(Period::Week, 0, &plays, &unlocks, NOW, TZ);
        assert_eq!(r.total_secs, 5 * 3600 + 1800);
        assert_eq!(r.previous_secs, 3600);
        assert_eq!((r.sessions, r.days_played, r.best_streak), (3, 3, 3));
        assert_eq!(r.top[0], (1, 5 * 3600, 2));
        assert_eq!(r.longest, Some((1, 3 * 3600, at(0, 21))));
        assert_eq!(r.weekday, Some(0));
        assert_eq!(r.day_part, Some(DayPart::Evening));
        assert_eq!(r.new_games, 1, "jeu 2 découvert cette semaine, jeu 1 déjà joué avant");
        assert_eq!(r.unlocks, 2);
        assert_eq!(r.rarest.as_ref().map(|u| u.name.as_str()), Some("Rare"));
        assert_eq!(r.days_in_period, 3, "lundi → mercredi écoulés");
        assert_eq!(r.profile, Profile::Loyal);
        let empty = compute(Period::Year, -5, &plays, &unlocks, NOW, TZ);
        assert_eq!((empty.total_secs, empty.profile, empty.weekday), (0, Profile::Casual, None));
    }

    #[test]
    fn profiles() {
        let night: Vec<Play> = (0..3).map(|d| Play { game: d as GameId, started_at: at(d, 1), secs: 3600 }).collect();
        assert_eq!(compute(Period::Week, 0, &night, &[], NOW, TZ).profile, Profile::NightOwl);
        let explorer: Vec<Play> = (0..7).map(|g| Play { game: g, started_at: at(0, 15), secs: 600 }).collect();
        assert_eq!(compute(Period::Week, 0, &explorer, &[], NOW, TZ).profile, Profile::Explorer);
    }
}

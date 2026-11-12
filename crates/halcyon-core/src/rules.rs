//! Collections intelligentes : règles évaluées automatiquement sur la bibliothèque.
//!
//! Une collection intelligente ne stocke aucun jeu : elle est recalculée à chaque
//! changement de la bibliothèque (scan, session, succès…), donc toujours à jour.

use crate::model::{Game, GameSource, PlayStatus};
use crate::search::{filter_matches, Filter, QueryContext, SearchQuery};
use crate::util;
use serde::{Deserialize, Serialize};

const DAY: i64 = 86_400;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Rule {
    NeverPlayed,
    PlayedWithinDays { days: u32 },
    NotPlayedForDays { days: u32 },
    InstalledWithinDays { days: u32 },
    AddedWithinDays { days: u32 },
    Installed,
    Favorite,
    Status { status: PlayStatus },
    AchievementsMissing,
    AchievementsComplete,
    AchievementPercentAtLeast { percent: u32 },
    Tag { tag: String },
    Genre { genre: String },
    Source { source: GameSource },
    Platform { platform: String },
    /// Dossier d'installation sous ce préfixe (« D: », « /mnt/jeux »).
    Drive { prefix: String },
    NeedsUpdate,
    PlaytimeAtLeastHours { hours: u32 },
    PlaytimeBelowHours { hours: u32 },
    /// Requête de recherche avancée (voir [`crate::search`]).
    Search { query: String },
    Not { rule: Box<Rule> },
}

/// Type de paramètre attendu par une règle (pour l'éditeur).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParamKind {
    None,
    Number,
    Status,
    Source,
    Text,
}

/// Catalogue des règles proposées dans l'éditeur, dans l'ordre d'affichage.
pub const KINDS: &[(&str, ParamKind)] = &[
    ("Jamais lancé", ParamKind::None),
    ("Joué ces N derniers jours", ParamKind::Number),
    ("Pas joué depuis N jours", ParamKind::Number),
    ("Installé ces N derniers jours", ParamKind::Number),
    ("Ajouté ces N derniers jours", ParamKind::Number),
    ("Installé", ParamKind::None),
    ("Favori", ParamKind::None),
    ("Statut", ParamKind::Status),
    ("Succès manquants", ParamKind::None),
    ("Tous les succès obtenus", ParamKind::None),
    ("Progression des succès ≥ N %", ParamKind::Number),
    ("Étiquette", ParamKind::Text),
    ("Genre", ParamKind::Text),
    ("Source", ParamKind::Source),
    ("Plateforme émulée", ParamKind::Text),
    ("Sur le disque / dossier", ParamKind::Text),
    ("Mise à jour requise", ParamKind::None),
    ("Temps de jeu ≥ N heures", ParamKind::Number),
    ("Temps de jeu < N heures", ParamKind::Number),
    ("Recherche avancée", ParamKind::Text),
];

impl Rule {
    /// Construit une règle depuis l'éditeur. `None` si le paramètre est invalide.
    pub fn from_parts(kind: usize, param: &str, negate: bool) -> Option<Rule> {
        let p = param.trim();
        let num = || p.parse::<u32>().ok();
        let text = || Some(p.to_string()).filter(|s| !s.is_empty());
        let rule = match kind {
            0 => Rule::NeverPlayed,
            1 => Rule::PlayedWithinDays { days: num()? },
            2 => Rule::NotPlayedForDays { days: num()? },
            3 => Rule::InstalledWithinDays { days: num()? },
            4 => Rule::AddedWithinDays { days: num()? },
            5 => Rule::Installed,
            6 => Rule::Favorite,
            7 => Rule::Status { status: PlayStatus::parse(p) },
            8 => Rule::AchievementsMissing,
            9 => Rule::AchievementsComplete,
            10 => Rule::AchievementPercentAtLeast { percent: num()?.min(100) },
            11 => Rule::Tag { tag: text()? },
            12 => Rule::Genre { genre: text()? },
            13 => Rule::Source { source: GameSource::parse(p)? },
            14 => Rule::Platform { platform: text()? },
            15 => Rule::Drive { prefix: text()? },
            16 => Rule::NeedsUpdate,
            17 => Rule::PlaytimeAtLeastHours { hours: num()? },
            18 => Rule::PlaytimeBelowHours { hours: num()? },
            19 => {
                let q = text()?;
                if !SearchQuery::parse(&q).errors.is_empty() {
                    return None;
                }
                Rule::Search { query: q }
            }
            _ => return None,
        };
        Some(if negate { Rule::Not { rule: Box::new(rule) } } else { rule })
    }

    /// Inverse de [`from_parts`](Self::from_parts) : (type, paramètre, négation).
    pub fn parts(&self) -> (usize, String, bool) {
        match self {
            Rule::Not { rule } => {
                let (k, p, n) = rule.parts();
                (k, p, !n)
            }
            Rule::NeverPlayed => (0, String::new(), false),
            Rule::PlayedWithinDays { days } => (1, days.to_string(), false),
            Rule::NotPlayedForDays { days } => (2, days.to_string(), false),
            Rule::InstalledWithinDays { days } => (3, days.to_string(), false),
            Rule::AddedWithinDays { days } => (4, days.to_string(), false),
            Rule::Installed => (5, String::new(), false),
            Rule::Favorite => (6, String::new(), false),
            Rule::Status { status } => (7, status.as_str().into(), false),
            Rule::AchievementsMissing => (8, String::new(), false),
            Rule::AchievementsComplete => (9, String::new(), false),
            Rule::AchievementPercentAtLeast { percent } => (10, percent.to_string(), false),
            Rule::Tag { tag } => (11, tag.clone(), false),
            Rule::Genre { genre } => (12, genre.clone(), false),
            Rule::Source { source } => (13, source.as_str().into(), false),
            Rule::Platform { platform } => (14, platform.clone(), false),
            Rule::Drive { prefix } => (15, prefix.clone(), false),
            Rule::NeedsUpdate => (16, String::new(), false),
            Rule::PlaytimeAtLeastHours { hours } => (17, hours.to_string(), false),
            Rule::PlaytimeBelowHours { hours } => (18, hours.to_string(), false),
            Rule::Search { query } => (19, query.clone(), false),
        }
    }

    /// Description lisible (« Joué ces 7 derniers jours »).
    pub fn describe(&self) -> String {
        match self {
            Rule::Not { rule } => format!("Sauf : {}", rule.describe().to_lowercase()),
            Rule::NeverPlayed => "Jamais lancé".into(),
            Rule::PlayedWithinDays { days } => format!("Joué ces {days} derniers jours"),
            Rule::NotPlayedForDays { days } => format!("Pas joué depuis {days} jours"),
            Rule::InstalledWithinDays { days } => format!("Installé ces {days} derniers jours"),
            Rule::AddedWithinDays { days } => format!("Ajouté ces {days} derniers jours"),
            Rule::Installed => "Installé".into(),
            Rule::Favorite => "Favori".into(),
            Rule::Status { status } => format!("Statut : {}", status.label()),
            Rule::AchievementsMissing => "Succès manquants".into(),
            Rule::AchievementsComplete => "Tous les succès obtenus".into(),
            Rule::AchievementPercentAtLeast { percent } => format!("Progression des succès ≥ {percent} %"),
            Rule::Tag { tag } => format!("Étiquette « {tag} »"),
            Rule::Genre { genre } => format!("Genre « {genre} »"),
            Rule::Source { source } => format!("Source : {}", source.label()),
            Rule::Platform { platform } => format!("Plateforme « {platform} »"),
            Rule::Drive { prefix } => format!("Sur {prefix}"),
            Rule::NeedsUpdate => "Mise à jour requise".into(),
            Rule::PlaytimeAtLeastHours { hours } => format!("Temps de jeu ≥ {hours} h"),
            Rule::PlaytimeBelowHours { hours } => format!("Temps de jeu < {hours} h"),
            Rule::Search { query } => format!("Recherche « {query} »"),
        }
    }

    pub fn matches(&self, g: &Game, ctx: &QueryContext) -> bool {
        let within = |t: Option<i64>, days: u32| t.is_some_and(|t| ctx.now - t <= days as i64 * DAY);
        let progress = ctx.progress.get(&g.id).filter(|p| p.total > 0);
        match self {
            Rule::Not { rule } => !rule.matches(g, ctx),
            Rule::NeverPlayed => g.last_played.is_none() && g.effective_playtime() == 0,
            Rule::PlayedWithinDays { days } => within(g.last_played, *days),
            Rule::NotPlayedForDays { days } => g.last_played.is_some() && !within(g.last_played, *days),
            Rule::InstalledWithinDays { days } => g.installed && within(g.installed_at.or(Some(g.added_at)), *days),
            Rule::AddedWithinDays { days } => within(Some(g.added_at), *days),
            Rule::Installed => g.installed,
            Rule::Favorite => g.favorite,
            Rule::Status { status } => g.status == *status,
            Rule::AchievementsMissing => progress.is_some_and(|p| !p.complete()),
            Rule::AchievementsComplete => progress.is_some_and(|p| p.complete()),
            Rule::AchievementPercentAtLeast { percent } => progress.is_some_and(|p| p.percent() >= *percent as f32),
            Rule::Tag { tag } => filter_matches(&Filter::Tag(util::search_key(tag)), g, ctx),
            Rule::Genre { genre } => filter_matches(&Filter::Genre(util::search_key(genre)), g, ctx),
            Rule::Source { source } => {
                g.source == *source || (*source == GameSource::Standalone && g.source == GameSource::Manual)
            }
            Rule::Platform { platform } => filter_matches(&Filter::Platform(util::search_key(platform)), g, ctx),
            Rule::Drive { prefix } => {
                let p = util::path_key(std::path::Path::new(prefix));
                !p.is_empty()
                    && g.install_dir.as_deref().is_some_and(|d| util::path_key(d).starts_with(&p))
            }
            Rule::NeedsUpdate => g.update.state.pending(),
            Rule::PlaytimeAtLeastHours { hours } => g.effective_playtime() >= *hours as u64 * 3600,
            Rule::PlaytimeBelowHours { hours } => g.effective_playtime() < *hours as u64 * 3600,
            Rule::Search { query } => SearchQuery::parse(query).matches(g, ctx).is_some(),
        }
    }
}

/// Ensemble de règles d'une collection intelligente.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RuleSet {
    /// `true` : toutes les règles doivent correspondre ; `false` : au moins une.
    pub match_all: bool,
    pub rules: Vec<Rule>,
}

impl RuleSet {
    pub fn all(rules: Vec<Rule>) -> Self {
        Self { match_all: true, rules }
    }

    pub fn matches(&self, g: &Game, ctx: &QueryContext) -> bool {
        if g.hidden || self.rules.is_empty() {
            return false;
        }
        if self.match_all {
            self.rules.iter().all(|r| r.matches(g, ctx))
        } else {
            self.rules.iter().any(|r| r.matches(g, ctx))
        }
    }

    pub fn describe(&self) -> String {
        let sep = if self.match_all { " et " } else { " ou " };
        let mut out = String::new();
        for (i, r) in self.rules.iter().enumerate() {
            let d = r.describe();
            if i == 0 {
                out.push_str(&d);
            } else {
                out.push_str(sep);
                let mut c = d.chars();
                if let Some(first) = c.next() {
                    out.extend(first.to_lowercase());
                    out.push_str(c.as_str());
                }
            }
        }
        out
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    pub fn from_json(s: &str) -> Option<Self> {
        serde_json::from_str(s).ok()
    }
}

/// Collections intelligentes proposées par défaut (modifiables et supprimables).
pub fn presets() -> Vec<(&'static str, RuleSet)> {
    use Rule::*;
    vec![
        ("Jamais lancés", RuleSet::all(vec![NeverPlayed, Installed])),
        ("Installés récemment", RuleSet::all(vec![InstalledWithinDays { days: 14 }])),
        ("Joués cette semaine", RuleSet::all(vec![PlayedWithinDays { days: 7 }])),
        ("Terminés", RuleSet::all(vec![Status { status: PlayStatus::Completed }])),
        (
            "À terminer",
            RuleSet {
                match_all: false,
                rules: vec![Status { status: PlayStatus::Playing }, Status { status: PlayStatus::Backlog }],
            },
        ),
        (
            "Succès presque complets",
            RuleSet::all(vec![AchievementPercentAtLeast { percent: 80 }, AchievementsMissing]),
        ),
        ("Succès manquants", RuleSet::all(vec![AchievementsMissing])),
        ("Multijoueur", RuleSet::all(vec![Tag { tag: "multijoueur".into() }])),
        ("Mise à jour requise", RuleSet::all(vec![NeedsUpdate])),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::achievements::Progress;
    use crate::model::UpdateState;

    #[test]
    fn presets_evaluate() {
        let now = 1_000 * DAY;
        let mut ctx = QueryContext { now, ..Default::default() };
        let mut a = Game::blank(1, "A", GameSource::Steam);
        a.added_at = now - 2 * DAY;
        a.install_dir = Some("D:\\Games\\A".into());
        let mut b = Game::blank(2, "B", GameSource::Epic);
        b.last_played = Some(now - 3 * DAY);
        b.playtime_secs = 3600;
        b.status = PlayStatus::Playing;
        b.update.state = UpdateState::Downloading;
        b.tags = vec!["Multijoueur".into()];
        ctx.progress.insert(2, Progress { unlocked: 9, total: 10 });

        let p: std::collections::HashMap<_, _> = presets().into_iter().collect();
        let hit = |name: &str, g: &Game| p[name].matches(g, &ctx);
        assert!(hit("Jamais lancés", &a) && !hit("Jamais lancés", &b));
        assert!(hit("Installés récemment", &a));
        assert!(hit("Joués cette semaine", &b) && !hit("Joués cette semaine", &a));
        assert!(hit("À terminer", &b));
        assert!(hit("Succès presque complets", &b));
        assert!(hit("Multijoueur", &b));
        assert!(hit("Mise à jour requise", &b));
        assert!(Rule::Drive { prefix: "d:".into() }.matches(&a, &ctx));
        a.hidden = true;
        assert!(!hit("Jamais lancés", &a), "les jeux masqués ne figurent pas dans les collections");
    }

    #[test]
    fn editor_round_trip_and_json() {
        for (kind, def) in KINDS.iter().enumerate() {
            let param = match def.1 {
                ParamKind::None => "",
                ParamKind::Number => "12",
                ParamKind::Status => "completed",
                ParamKind::Source => "steam",
                ParamKind::Text => "rpg",
            };
            let r = Rule::from_parts(kind, param, kind % 2 == 0).unwrap();
            assert_eq!(r.parts(), (kind, param.to_string(), kind % 2 == 0));
            assert!(!r.describe().is_empty());
        }
        assert!(Rule::from_parts(1, "abc", false).is_none(), "nombre invalide refusé");
        assert!(Rule::from_parts(19, "statut:n'importe", false).is_none());
        let set = presets().remove(4).1;
        assert_eq!(RuleSet::from_json(&set.to_json()), Some(set.clone()));
        assert_eq!(set.describe(), "Statut : En cours ou statut : À jouer");
    }
}

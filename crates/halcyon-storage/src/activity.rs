//! Journal d'activité : historique lisible des événements (sessions, succès,
//! installations, mises à jour, sauvegardes…), distinct des journaux techniques.

use crate::{now_unix, Database, Result};
use halcyon_core::util;
use halcyon_core::GameId;
use rusqlite::params;

/// Nombre maximal d'entrées conservées (les plus anciennes sont purgées).
const MAX_ENTRIES: i64 = 20_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ActivityKind {
    Launch,
    Session,
    Achievement,
    Added,
    Installed,
    Uninstalled,
    Update,
    Save,
    Mod,
    Profile,
    Status,
    Settings,
    Plugin,
    Sync,
    Error,
}

/// Catégories proposées comme filtres dans le journal.
pub const CATEGORIES: &[(&str, &[ActivityKind])] = &[
    (tr_noop!("Sessions"), &[ActivityKind::Launch, ActivityKind::Session]),
    (tr_noop!("Succès"), &[ActivityKind::Achievement]),
    (tr_noop!("Bibliothèque"), &[ActivityKind::Added, ActivityKind::Installed, ActivityKind::Uninstalled, ActivityKind::Status]),
    (tr_noop!("Mises à jour"), &[ActivityKind::Update]),
    (tr_noop!("Sauvegardes"), &[ActivityKind::Save]),
    (tr_noop!("Mods"), &[ActivityKind::Mod]),
    (
        tr_noop!("Système"),
        &[ActivityKind::Profile, ActivityKind::Settings, ActivityKind::Plugin, ActivityKind::Sync, ActivityKind::Error],
    ),
];

impl ActivityKind {
    const ALL: [ActivityKind; 15] = [
        Self::Launch,
        Self::Session,
        Self::Achievement,
        Self::Added,
        Self::Installed,
        Self::Uninstalled,
        Self::Update,
        Self::Save,
        Self::Mod,
        Self::Profile,
        Self::Status,
        Self::Settings,
        Self::Plugin,
        Self::Sync,
        Self::Error,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Launch => "launch",
            Self::Session => "session",
            Self::Achievement => "achievement",
            Self::Added => "added",
            Self::Installed => "installed",
            Self::Uninstalled => "uninstalled",
            Self::Update => "update",
            Self::Save => "save",
            Self::Mod => "mod",
            Self::Profile => "profile",
            Self::Status => "status",
            Self::Settings => "settings",
            Self::Plugin => "plugin",
            Self::Sync => "sync",
            Self::Error => "error",
        }
    }

    pub fn parse(s: &str) -> Self {
        Self::ALL.into_iter().find(|k| k.as_str() == s).unwrap_or(Self::Settings)
    }

    /// Index de la catégorie (filtres du journal).
    pub fn category(self) -> usize {
        CATEGORIES.iter().position(|(_, kinds)| kinds.contains(&self)).unwrap_or(CATEGORIES.len() - 1)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Activity {
    pub id: i64,
    pub at: i64,
    pub kind: ActivityKind,
    pub game_id: Option<GameId>,
    pub title: String,
    pub detail: String,
}

impl Database {
    pub fn log_activity(&self, kind: ActivityKind, game: Option<GameId>, title: &str, detail: &str) -> Result<()> {
        log_with(&self.conn, kind, game, title, detail)
    }

    /// Entrées les plus récentes, filtrées par catégories (vide = toutes) et texte.
    pub fn activities(&self, categories: &[usize], search: &str, limit: usize) -> Result<Vec<Activity>> {
        let needle = util::search_key(search);
        let tokens: Vec<&str> = needle.split(' ').filter(|t| !t.is_empty()).collect();
        let mut stmt =
            self.conn.prepare("SELECT id, at, kind, game_id, title, detail FROM activity ORDER BY at DESC, id DESC")?;
        let rows = stmt.query_map([], |r| {
            Ok(Activity {
                id: r.get(0)?,
                at: r.get(1)?,
                kind: ActivityKind::parse(&r.get::<_, String>(2)?),
                game_id: r.get(3)?,
                // textes enregistrés avec tr_later! : traduits dans la langue active
                title: halcyon_i18n::render(&r.get::<_, String>(4)?),
                detail: halcyon_i18n::render(&r.get::<_, String>(5)?),
            })
        })?;
        let mut out = Vec::new();
        for a in rows {
            let a = a?;
            if !categories.is_empty() && !categories.contains(&a.kind.category()) {
                continue;
            }
            if !tokens.is_empty() {
                let hay = util::search_key(&format!("{} {}", a.title, a.detail));
                if !tokens.iter().all(|t| hay.contains(t)) {
                    continue;
                }
            }
            out.push(a);
            if out.len() >= limit {
                break;
            }
        }
        Ok(out)
    }

    pub fn activity_count(&self) -> Result<i64> {
        Ok(self.conn.query_row("SELECT COUNT(*) FROM activity", [], |r| r.get(0))?)
    }
}

pub(crate) fn log_with(
    conn: &rusqlite::Connection,
    kind: ActivityKind,
    game: Option<GameId>,
    title: &str,
    detail: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO activity (at, kind, game_id, title, detail) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![now_unix(), kind.as_str(), game, title, detail],
    )?;
    // Purge occasionnelle (une insertion sur 500) pour borner la taille.
    if conn.last_insert_rowid() % 500 == 0 {
        conn.execute(
            "DELETE FROM activity WHERE id <= (SELECT id FROM activity ORDER BY id DESC LIMIT 1 OFFSET ?1)",
            [MAX_ENTRIES],
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_by_category_and_text() {
        let db = Database::open_in_memory().unwrap();
        db.log_activity(ActivityKind::Session, None, "Session de Hadès", "1 h 20").unwrap();
        db.log_activity(ActivityKind::Achievement, None, "Succès débloqué", "Hadès · Évasion").unwrap();
        db.log_activity(ActivityKind::Update, None, "Mise à jour de Celeste", "1.4 → 1.5").unwrap();
        assert_eq!(db.activities(&[], "", 10).unwrap().len(), 3);
        assert_eq!(db.activities(&[0], "", 10).unwrap().len(), 1);
        let hits = db.activities(&[], "hades", 10).unwrap();
        assert_eq!(hits.len(), 2, "recherche insensible aux accents");
        assert_eq!(db.activities(&[1, 3], "", 10).unwrap().len(), 2);
        assert_eq!(db.activities(&[], "", 1).unwrap().len(), 1);
        assert_eq!(ActivityKind::Uninstalled.category(), 2);
    }
}

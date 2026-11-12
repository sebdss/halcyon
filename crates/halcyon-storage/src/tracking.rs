//! Suivi personnel (statut, notes, temps corrigé), versions, profils de lancement,
//! collections intelligentes et données brutes des statistiques.

use crate::activity::ActivityKind;
use crate::{now_unix, Database, Result, StorageError};
use halcyon_core::profiles::LaunchProfile;
use halcyon_core::rules::RuleSet;
use halcyon_core::{util, GameId, PlayStatus, UpdateInfo};
use rusqlite::{params, OptionalExtension};

/// Session de jeu mesurée localement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Session {
    pub game_id: GameId,
    pub started_at: i64,
    pub duration_secs: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SmartCollection {
    pub id: i64,
    pub name: String,
    pub rules: RuleSet,
}

/// Changement de version récent (page Mises à jour).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VersionChange {
    pub game_id: GameId,
    pub game: String,
    pub version: String,
    pub seen_at: i64,
}

impl Database {
    // ------------------------------------------------------------ suivi personnel

    pub fn set_status(&self, id: GameId, status: PlayStatus) -> Result<()> {
        let (name, old): (String, String) =
            self.conn.query_row("SELECT name, status FROM games WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?)))?;
        if PlayStatus::parse(&old) == status {
            return Ok(());
        }
        self.conn.execute(
            "UPDATE games SET status = ?2,
                completed_at = CASE WHEN ?2 = 'completed' THEN COALESCE(completed_at, ?3) ELSE NULL END
             WHERE id = ?1",
            params![id, status.as_str(), now_unix()],
        )?;
        let title = match status {
            PlayStatus::Completed => format!("{name} terminé"),
            PlayStatus::None => format!("Statut retiré pour {name}"),
            s => format!("{name} : {}", s.label().to_lowercase()),
        };
        self.log_activity(ActivityKind::Status, Some(id), &title, "")
    }

    pub fn set_notes(&self, id: GameId, notes: &str) -> Result<()> {
        self.conn.execute("UPDATE games SET notes = ?2 WHERE id = ?1", params![id, notes])?;
        Ok(())
    }

    /// Correction manuelle du temps de jeu (`None` : revenir au temps calculé).
    pub fn set_manual_playtime(&self, id: GameId, secs: Option<u64>) -> Result<()> {
        self.conn.execute(
            "UPDATE games SET playtime_manual_secs = ?2 WHERE id = ?1",
            params![id, secs.map(|v| v as i64)],
        )?;
        let name: String = self.conn.query_row("SELECT name FROM games WHERE id = ?1", [id], |r| r.get(0))?;
        let detail = match secs {
            Some(s) => format!("Temps corrigé : {}", util::format_playtime(s)),
            None => "Retour au temps calculé".into(),
        };
        self.log_activity(ActivityKind::Status, Some(id), &format!("Temps de jeu de {name} modifié"), &detail)
    }

    /// Progression d'une mise à jour en cours (relevée pendant le téléchargement).
    pub fn set_update_progress(&self, id: GameId, u: UpdateInfo) -> Result<()> {
        self.conn.execute(
            "UPDATE games SET update_state = ?2, update_downloaded = ?3, update_total = ?4 WHERE id = ?1",
            params![id, u.state.as_str(), u.downloaded as i64, u.total as i64],
        )?;
        Ok(())
    }

    pub fn version_history(&self, id: GameId) -> Result<Vec<(String, i64)>> {
        let mut stmt =
            self.conn.prepare("SELECT version, seen_at FROM version_history WHERE game_id = ?1 ORDER BY seen_at DESC, id DESC")?;
        let rows = stmt.query_map([id], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Changements de version observés (hors première détection d'un jeu).
    pub fn recent_version_changes(&self, limit: usize) -> Result<Vec<VersionChange>> {
        let mut stmt = self.conn.prepare(
            "SELECT v.game_id, g.name, v.version, v.seen_at FROM version_history v
             JOIN games g ON g.id = v.game_id
             WHERE EXISTS (SELECT 1 FROM version_history p WHERE p.game_id = v.game_id AND p.id < v.id)
             ORDER BY v.seen_at DESC, v.id DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit as i64], |r| {
            Ok(VersionChange { game_id: r.get(0)?, game: r.get(1)?, version: r.get(2)?, seen_at: r.get(3)? })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    // ------------------------------------------------------------ statistiques

    /// Toutes les sessions mesurées localement, des plus récentes aux plus anciennes.
    pub fn sessions(&self) -> Result<Vec<Session>> {
        let mut stmt =
            self.conn.prepare("SELECT game_id, started_at, duration_secs FROM play_sessions ORDER BY started_at DESC")?;
        let rows = stmt.query_map([], |r| {
            Ok(Session { game_id: r.get(0)?, started_at: r.get(1)?, duration_secs: r.get::<_, i64>(2)?.max(0) as u64 })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Sessions commencées depuis `since` (horodatage Unix), sans lire tout l'historique.
    pub fn sessions_since(&self, since: i64) -> Result<Vec<Session>> {
        let mut stmt = self.conn.prepare(
            "SELECT game_id, started_at, duration_secs FROM play_sessions WHERE started_at >= ?1 ORDER BY started_at DESC",
        )?;
        let rows = stmt.query_map([since], |r| {
            Ok(Session { game_id: r.get(0)?, started_at: r.get(1)?, duration_secs: r.get::<_, i64>(2)?.max(0) as u64 })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Première et dernière session enregistrées pour un jeu.
    pub fn session_bounds(&self, id: GameId) -> Result<Option<(i64, i64)>> {
        Ok(self
            .conn
            .query_row(
                "SELECT MIN(started_at), MAX(started_at + duration_secs) FROM play_sessions WHERE game_id = ?1",
                [id],
                |r| Ok((r.get::<_, Option<i64>>(0)?, r.get::<_, Option<i64>>(1)?)),
            )
            .optional()?
            .and_then(|(a, b)| Some((a?, b?))))
    }

    /// Dates de déblocage de tous les succès (plateformes et personnalisés).
    pub fn unlock_times(&self) -> Result<Vec<i64>> {
        let mut stmt = self.conn.prepare("SELECT unlocked_at FROM achievements WHERE unlocked = 1 AND unlocked_at IS NOT NULL")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    // ------------------------------------------------------------ profils de lancement

    pub fn launch_profiles(&self, game: GameId) -> Result<Vec<LaunchProfile>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, is_default, data FROM launch_profiles WHERE game_id = ?1 ORDER BY id")?;
        let rows = stmt.query_map([game], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, bool>(2)?, r.get::<_, String>(3)?))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, name, is_default, data) = row?;
            let mut p: LaunchProfile = serde_json::from_str(&data).unwrap_or_default();
            p.id = id;
            p.game_id = game;
            p.name = name;
            p.is_default = is_default;
            out.push(p);
        }
        Ok(out)
    }

    /// Crée (`id == 0`) ou met à jour un profil ; renvoie son identifiant.
    pub fn save_launch_profile(&self, p: &LaunchProfile) -> Result<i64> {
        let name = p.name.trim();
        if name.is_empty() {
            return Err(StorageError::Conflict("le profil doit avoir un nom".into()));
        }
        let data = serde_json::to_string(p)?;
        let id = if p.id == 0 {
            self.conn.execute(
                "INSERT INTO launch_profiles (game_id, name, is_default, data) VALUES (?1, ?2, ?3, ?4)",
                params![p.game_id, name, p.is_default, data],
            )?;
            self.conn.last_insert_rowid()
        } else {
            self.conn.execute(
                "UPDATE launch_profiles SET name = ?2, is_default = ?3, data = ?4 WHERE id = ?1",
                params![p.id, name, p.is_default, data],
            )?;
            p.id
        };
        if p.is_default {
            self.conn.execute(
                "UPDATE launch_profiles SET is_default = 0 WHERE game_id = ?1 AND id != ?2",
                params![p.game_id, id],
            )?;
        }
        Ok(id)
    }

    pub fn delete_launch_profile(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM launch_profiles WHERE id = ?1", [id])?;
        Ok(())
    }

    // ------------------------------------------------------------ collections intelligentes

    pub fn smart_collections(&self) -> Result<Vec<SmartCollection>> {
        let mut stmt = self.conn.prepare("SELECT id, name, rules FROM smart_collections ORDER BY position, id")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?;
        let mut out = Vec::new();
        for row in rows {
            let (id, name, json) = row?;
            match RuleSet::from_json(&json) {
                Some(rules) => out.push(SmartCollection { id, name, rules }),
                None => log::warn!("collection intelligente « {name} » illisible, ignorée"),
            }
        }
        Ok(out)
    }

    /// Crée (`id == 0`) ou modifie une collection intelligente.
    pub fn save_smart_collection(&self, id: i64, name: &str, rules: &RuleSet) -> Result<i64> {
        let name = name.trim();
        if name.is_empty() {
            return Err(StorageError::Conflict("le nom de la collection est vide".into()));
        }
        if rules.rules.is_empty() {
            return Err(StorageError::Conflict("ajoutez au moins une règle".into()));
        }
        let clash: Option<i64> = self
            .conn
            .query_row("SELECT id FROM smart_collections WHERE name = ?1 AND id != ?2", params![name, id], |r| r.get(0))
            .optional()?;
        if clash.is_some() {
            return Err(StorageError::Conflict(format!("une collection « {name} » existe déjà")));
        }
        if id == 0 {
            self.conn.execute(
                "INSERT INTO smart_collections (name, rules, position)
                 VALUES (?1, ?2, (SELECT COALESCE(MAX(position), 0) + 1 FROM smart_collections))",
                params![name, rules.to_json()],
            )?;
            Ok(self.conn.last_insert_rowid())
        } else {
            self.conn.execute(
                "UPDATE smart_collections SET name = ?2, rules = ?3 WHERE id = ?1",
                params![id, name, rules.to_json()],
            )?;
            Ok(id)
        }
    }

    pub fn delete_smart_collection(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM smart_collections WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Crée les collections intelligentes par défaut si aucune n'existe.
    pub fn seed_smart_collections(&self) -> Result<()> {
        let n: i64 = self.conn.query_row("SELECT COUNT(*) FROM smart_collections", [], |r| r.get(0))?;
        if n == 0 {
            for (name, rules) in halcyon_core::rules::presets() {
                self.save_smart_collection(0, name, &rules)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use halcyon_core::profiles::DisplayMode;
    use halcyon_core::{DiscoveredGame, GameSource, ScanReport, UpdateState};

    fn one_game(db: &mut Database) -> GameId {
        let mut d = DiscoveredGame::new("1", "Hadès");
        d.version = Some("1.0".into());
        d.playtime_secs = Some(7200);
        db.merge_scan(GameSource::Steam, &ScanReport { games: vec![d], warnings: vec![], complete: true }).unwrap();
        db.load_games().unwrap()[0].id
    }

    #[test]
    fn status_notes_playtime_and_versions() {
        let mut db = Database::open_in_memory().unwrap();
        let id = one_game(&mut db);
        let g = db.get_game(id).unwrap();
        assert_eq!(g.playtime_platform_secs, Some(7200));
        assert!(g.installed_at.is_some());

        db.set_status(id, PlayStatus::Completed).unwrap();
        db.set_notes(id, "Fin 1 vue").unwrap();
        db.set_manual_playtime(id, Some(9000)).unwrap();
        let g = db.get_game(id).unwrap();
        assert_eq!(g.status, PlayStatus::Completed);
        assert!(g.completed_at.is_some());
        assert_eq!(g.effective_playtime(), 9000);
        assert_eq!(g.notes, "Fin 1 vue");
        db.set_status(id, PlayStatus::Playing).unwrap();
        assert!(db.get_game(id).unwrap().completed_at.is_none());

        // Nouvelle version + mise à jour en attente.
        let mut d = DiscoveredGame::new("1", "Hadès");
        d.version = Some("1.1".into());
        d.update = Some(UpdateInfo { state: UpdateState::Available, downloaded: 0, total: 100 });
        let s = db.merge_scan(GameSource::Steam, &ScanReport { games: vec![d.clone()], warnings: vec![], complete: true }).unwrap();
        assert_eq!(s.new_updates, [(id, "Hadès".to_string())]);
        let s = db.merge_scan(GameSource::Steam, &ScanReport { games: vec![d], warnings: vec![], complete: true }).unwrap();
        assert!(s.new_updates.is_empty(), "signalée une seule fois");
        assert_eq!(db.version_history(id).unwrap().len(), 2);
        let changes = db.recent_version_changes(10).unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].version, "1.1");
        let journal = db.activities(&[], "", 50).unwrap();
        assert!(journal.iter().any(|a| a.detail == "1.0 → 1.1"));
        assert!(journal.iter().any(|a| a.title == "Hadès terminé"));
    }

    #[test]
    fn profiles_and_smart_collections() {
        let mut db = Database::open_in_memory().unwrap();
        let id = one_game(&mut db);
        let mut p = LaunchProfile::new(id, "Fenêtré");
        p.display = DisplayMode::Windowed;
        p.is_default = true;
        let a = db.save_launch_profile(&p).unwrap();
        let b = db.save_launch_profile(&LaunchProfile { is_default: true, ..LaunchProfile::new(id, "Perf") }).unwrap();
        let list = db.launch_profiles(id).unwrap();
        assert_eq!(list.len(), 2);
        assert!(!list.iter().find(|p| p.id == a).unwrap().is_default, "un seul profil par défaut");
        assert!(list.iter().find(|p| p.id == b).unwrap().is_default);
        assert_eq!(list[0].display, DisplayMode::Windowed);
        db.delete_launch_profile(a).unwrap();
        assert_eq!(db.launch_profiles(id).unwrap().len(), 1);

        db.seed_smart_collections().unwrap();
        let n = db.smart_collections().unwrap().len();
        assert!(n >= 5);
        db.seed_smart_collections().unwrap();
        assert_eq!(db.smart_collections().unwrap().len(), n, "pas de doublon");
        let first = db.smart_collections().unwrap().remove(0);
        assert!(db.save_smart_collection(0, &first.name, &first.rules).is_err(), "nom unique");
        assert!(db.save_smart_collection(0, "Vide", &RuleSet::all(vec![])).is_err());
    }
}

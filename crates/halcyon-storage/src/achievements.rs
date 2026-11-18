//! Stockage des succès : fusion incrémentale, succès personnalisés, statistiques.

use crate::{now_unix, Database, Result, StorageError};
use halcyon_core::achievements::{Achievement, AchievementData, AchievementProvider, Progress, SyncStatus};
use halcyon_core::GameId;
use rusqlite::{params, OptionalExtension, Row};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const COLUMNS: &str = "id, game_id, provider, api_name, name, description, icon_url, icon_path, hidden, points,
    rarity, unlocked, unlocked_at, unlocked_hardcore_at, progress_cur, progress_max, display_order";

fn row_to_achievement(r: &Row) -> rusqlite::Result<Achievement> {
    let provider: String = r.get(2)?;
    let cur: Option<f64> = r.get(14)?;
    let max: Option<f64> = r.get(15)?;
    Ok(Achievement {
        id: r.get(0)?,
        game_id: r.get(1)?,
        provider: AchievementProvider::parse(&provider).unwrap_or(AchievementProvider::Custom),
        icon_path: r.get::<_, Option<String>>(7)?.map(PathBuf::from),
        data: AchievementData {
            api_name: r.get(3)?,
            name: r.get(4)?,
            description: r.get(5)?,
            icon_url: r.get(6)?,
            hidden: r.get(8)?,
            points: r.get::<_, Option<i64>>(9)?.map(|p| p.max(0) as u32),
            rarity: r.get::<_, Option<f64>>(10)?.map(|v| v as f32),
            unlocked: r.get(11)?,
            unlocked_at: r.get(12)?,
            unlocked_hardcore_at: r.get(13)?,
            progress: cur.zip(max),
            display_order: r.get(16)?,
        },
    })
}

/// Statistiques globales des succès.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AchievementStats {
    pub unlocked: u32,
    pub total: u32,
    pub games_with_achievements: u32,
    pub completed_games: Vec<GameId>,
    pub by_provider: Vec<(AchievementProvider, Progress)>,
    /// Déblocages par mois (clé : « AAAA-MM »), 12 derniers mois avec activité.
    pub per_month: Vec<(String, u32)>,
    pub points: u32,
}

impl Database {
    /// Fusionne la liste complète d'un jeu pour une plateforme.
    ///
    /// Renvoie les succès **nouvellement débloqués** (pour les notifications).
    /// Les icônes déjà téléchargées sont conservées ; les succès disparus de la
    /// plateforme sont retirés (la liste reçue est complète).
    pub fn merge_achievements(
        &mut self,
        game: GameId,
        provider: AchievementProvider,
        list: &[AchievementData],
    ) -> Result<Vec<Achievement>> {
        if provider == AchievementProvider::Custom {
            return Err(StorageError::Conflict(tr!("les succès personnalisés ne se synchronisent pas")));
        }
        let tx = self.conn.transaction()?;
        let previous: HashMap<String, bool> = {
            let mut stmt = tx.prepare("SELECT api_name, unlocked FROM achievements WHERE game_id = ?1 AND provider = ?2")?;
            let rows = stmt.query_map(params![game, provider.as_str()], |r| Ok((r.get(0)?, r.get(1)?)))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let first_sync = previous.is_empty();
        let mut newly = Vec::new();
        {
            let mut up = tx.prepare(
                "INSERT INTO achievements (game_id, provider, api_name, name, description, icon_url, hidden, points, rarity,
                    unlocked, unlocked_at, unlocked_hardcore_at, progress_cur, progress_max, display_order)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
                 ON CONFLICT (game_id, provider, api_name) DO UPDATE SET
                    name = excluded.name, description = excluded.description,
                    icon_path = CASE WHEN icon_url IS excluded.icon_url THEN icon_path ELSE NULL END,
                    icon_url = excluded.icon_url, hidden = excluded.hidden, points = excluded.points,
                    rarity = COALESCE(excluded.rarity, rarity), unlocked = excluded.unlocked,
                    unlocked_at = excluded.unlocked_at, unlocked_hardcore_at = excluded.unlocked_hardcore_at,
                    progress_cur = excluded.progress_cur, progress_max = excluded.progress_max,
                    display_order = excluded.display_order",
            )?;
            for a in list {
                up.execute(params![
                    game,
                    provider.as_str(),
                    a.api_name,
                    a.name,
                    a.description,
                    a.icon_url,
                    a.hidden,
                    a.points,
                    a.rarity.map(|r| r as f64),
                    a.unlocked,
                    a.unlocked_at,
                    a.unlocked_hardcore_at,
                    a.progress.map(|p| p.0),
                    a.progress.map(|p| p.1),
                    a.display_order,
                ])?;
                // Pas de notification en rafale lors de la toute première synchronisation.
                if a.unlocked && !first_sync && !previous.get(&a.api_name).copied().unwrap_or(false) {
                    newly.push(a.api_name.clone());
                }
            }
            let keep: std::collections::HashSet<&str> = list.iter().map(|a| a.api_name.as_str()).collect();
            let mut del = tx.prepare("DELETE FROM achievements WHERE game_id = ?1 AND provider = ?2 AND api_name = ?3")?;
            for name in previous.keys().filter(|k| !keep.contains(k.as_str())) {
                del.execute(params![game, provider.as_str(), name])?;
            }
        }
        tx.commit()?;
        self.set_sync_status(game, provider, if list.is_empty() { &SyncStatus::NoAchievements } else { &SyncStatus::Ok })?;
        let all = self.load_achievements(game)?;
        let unlocked: Vec<Achievement> =
            all.into_iter().filter(|a| a.provider == provider && newly.contains(&a.data.api_name)).collect();
        if !unlocked.is_empty() {
            let name: String = self.conn.query_row("SELECT name FROM games WHERE id = ?1", [game], |r| r.get(0))?;
            for a in &unlocked {
                self.log_activity(
                    crate::ActivityKind::Achievement,
                    Some(game),
                    &tr_later!("Succès débloqué : {name}", name = a.data.name),
                    &tr_later!("{name} · {provider}", name = name, provider = provider_label(provider)),
                )?;
            }
        }
        Ok(unlocked)
    }

    pub fn load_achievements(&self, game: GameId) -> Result<Vec<Achievement>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {COLUMNS} FROM achievements WHERE game_id = ?1 ORDER BY provider, unlocked DESC, unlocked_at DESC, display_order, id"
        ))?;
        let rows = stmt.query_map([game], row_to_achievement)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn set_achievement_icon(&self, id: i64, path: &Path) -> Result<()> {
        self.conn.execute("UPDATE achievements SET icon_path = ?2 WHERE id = ?1", params![id, path.to_string_lossy()])?;
        Ok(())
    }

    pub fn set_sync_status(&self, game: GameId, provider: AchievementProvider, status: &SyncStatus) -> Result<()> {
        let msg = match status {
            SyncStatus::Error(m) => Some(m.as_str()),
            _ => None,
        };
        self.conn.execute(
            "INSERT INTO achievement_sync (game_id, provider, synced_at, status, message) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (game_id, provider) DO UPDATE SET synced_at = ?3, status = ?4, message = ?5",
            params![game, provider.as_str(), now_unix(), status.as_str(), msg],
        )?;
        Ok(())
    }

    /// Dernière synchronisation : (horodatage, statut).
    pub fn sync_status(&self, game: GameId, provider: AchievementProvider) -> Result<Option<(i64, SyncStatus)>> {
        Ok(self
            .conn
            .query_row(
                "SELECT synced_at, status, message FROM achievement_sync WHERE game_id = ?1 AND provider = ?2",
                params![game, provider.as_str()],
                |r| {
                    let status: String = r.get(1)?;
                    let msg: Option<String> = r.get(2)?;
                    let s = match status.as_str() {
                        "ok" => SyncStatus::Ok,
                        "none" => SyncStatus::NoAchievements,
                        "unknown" => SyncStatus::NotRecognized,
                        "notcached" => SyncStatus::NotCached,
                        _ => SyncStatus::Error(msg.unwrap_or_default()),
                    };
                    Ok((r.get(0)?, s))
                },
            )
            .optional()?)
    }

    /// Horodatages de dernière synchronisation par jeu (pour la synchronisation incrémentale).
    pub fn sync_times(&self, provider: AchievementProvider) -> Result<HashMap<GameId, i64>> {
        let mut stmt = self.conn.prepare("SELECT game_id, synced_at FROM achievement_sync WHERE provider = ?1")?;
        let rows = stmt.query_map([provider.as_str()], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    // ---------------------------------------------------------------- succès personnalisés

    pub fn add_custom_achievement(&self, game: GameId, name: &str, description: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() {
            return Err(StorageError::Conflict(tr!("le nom du succès est vide")));
        }
        let order: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(display_order), 0) + 1 FROM achievements WHERE game_id = ?1 AND provider = 'custom'",
            [game],
            |r| r.get(0),
        )?;
        self.conn.execute(
            "INSERT INTO achievements (game_id, provider, api_name, name, description, display_order)
             VALUES (?1, 'custom', ?2, ?3, ?4, ?5)",
            params![game, format!("custom-{}-{order}", now_unix()), name, description.trim(), order],
        )?;
        Ok(())
    }

    pub fn set_custom_unlocked(&self, id: i64, unlocked: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE achievements SET unlocked = ?2, unlocked_at = CASE WHEN ?2 THEN ?3 ELSE NULL END
             WHERE id = ?1 AND provider = 'custom'",
            params![id, unlocked, now_unix()],
        )?;
        if unlocked {
            let (game, name, ach): (GameId, String, String) = self.conn.query_row(
                "SELECT a.game_id, g.name, a.name FROM achievements a JOIN games g ON g.id = a.game_id WHERE a.id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )?;
            self.log_activity(
                crate::ActivityKind::Achievement,
                Some(game),
                &tr_later!("Objectif personnel atteint : {ach}", ach = ach),
                &tr_later!("{name} · personnalisé", name = name),
            )?;
        }
        Ok(())
    }

    pub fn delete_custom_achievement(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM achievements WHERE id = ?1 AND provider = 'custom'", [id])?;
        Ok(())
    }

    // ---------------------------------------------------------------- statistiques

    /// Progression (débloqués / total) par jeu.
    pub fn progress_by_game(&self) -> Result<HashMap<GameId, Progress>> {
        let mut stmt = self.conn.prepare("SELECT game_id, SUM(unlocked), COUNT(*) FROM achievements GROUP BY game_id")?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get(0)?, Progress { unlocked: r.get::<_, i64>(1)? as u32, total: r.get::<_, i64>(2)? as u32 }))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn achievement_stats(&self) -> Result<AchievementStats> {
        let mut s = AchievementStats::default();
        let per_game = self.progress_by_game()?;
        s.games_with_achievements = per_game.len() as u32;
        for (id, p) in &per_game {
            s.unlocked += p.unlocked;
            s.total += p.total;
            if p.complete() {
                s.completed_games.push(*id);
            }
        }
        let mut stmt = self.conn.prepare("SELECT provider, SUM(unlocked), COUNT(*) FROM achievements GROUP BY provider")?;
        let rows = stmt.query_map([], |r| {
            let p: String = r.get(0)?;
            Ok((p, Progress { unlocked: r.get::<_, i64>(1)? as u32, total: r.get::<_, i64>(2)? as u32 }))
        })?;
        for row in rows {
            let (p, prog) = row?;
            if let Some(p) = AchievementProvider::parse(&p) {
                s.by_provider.push((p, prog));
            }
        }
        let mut stmt = self.conn.prepare(
            "SELECT strftime('%Y-%m', unlocked_at, 'unixepoch') AS m, COUNT(*) FROM achievements
             WHERE unlocked = 1 AND unlocked_at IS NOT NULL GROUP BY m ORDER BY m DESC LIMIT 12",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u32)))?;
        s.per_month = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        s.per_month.reverse();
        s.points = self.conn.query_row(
            "SELECT COALESCE(SUM(points), 0) FROM achievements WHERE unlocked = 1 AND provider = 'retroachievements'",
            [],
            |r| r.get::<_, i64>(0),
        )? as u32;
        Ok(s)
    }

    /// Derniers succès débloqués, toutes plateformes.
    pub fn recent_unlocks(&self, limit: usize) -> Result<Vec<Achievement>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {COLUMNS} FROM achievements WHERE unlocked = 1 AND unlocked_at IS NOT NULL
             ORDER BY unlocked_at DESC LIMIT ?1"
        ))?;
        let rows = stmt.query_map([limit as i64], row_to_achievement)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Succès débloqués entre `from` et `to` (exclu), du plus ancien au plus récent.
    pub fn unlocks_between(&self, from: i64, to: i64) -> Result<Vec<Achievement>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {COLUMNS} FROM achievements WHERE unlocked = 1 AND unlocked_at >= ?1 AND unlocked_at < ?2 ORDER BY unlocked_at"
        ))?;
        let rows = stmt.query_map([from, to], row_to_achievement)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Succès débloqués les plus rares (rareté connue uniquement).
    pub fn rarest_unlocks(&self, limit: usize) -> Result<Vec<Achievement>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {COLUMNS} FROM achievements WHERE unlocked = 1 AND rarity IS NOT NULL ORDER BY rarity ASC LIMIT ?1"
        ))?;
        let rows = stmt.query_map([limit as i64], row_to_achievement)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}

fn provider_label(p: AchievementProvider) -> String {
    match p {
        AchievementProvider::Steam => "Steam".into(),
        AchievementProvider::RetroAchievements => "RetroAchievements".into(),
        AchievementProvider::Custom => tr!("personnalisé"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use halcyon_core::{DiscoveredGame, GameSource, ScanReport};

    fn data(name: &str, unlocked: bool, at: Option<i64>) -> AchievementData {
        AchievementData { api_name: name.into(), name: name.into(), unlocked, unlocked_at: at, ..Default::default() }
    }

    fn setup() -> (Database, GameId) {
        let mut db = Database::open_in_memory().unwrap();
        let r = ScanReport { games: vec![DiscoveredGame::new("1", "G")], warnings: vec![], complete: true };
        db.merge_scan(GameSource::Steam, &r).unwrap();
        let id = db.load_games().unwrap()[0].id;
        (db, id)
    }

    #[test]
    fn merge_detects_new_unlocks_and_keeps_icons() {
        let (mut db, id) = setup();
        let mut list = vec![data("A", true, Some(100)), data("B", false, None), data("C", false, None)];
        list[0].icon_url = Some("http://i/a.jpg".into());
        assert!(db.merge_achievements(id, AchievementProvider::Steam, &list).unwrap().is_empty(), "1re synchro : silence");
        let a = db.load_achievements(id).unwrap().into_iter().find(|a| a.data.api_name == "A").unwrap();
        db.set_achievement_icon(a.id, Path::new("/cache/a.jpg")).unwrap();

        list[1] = data("B", true, Some(200));
        list.pop(); // C retiré par la plateforme
        let newly = db.merge_achievements(id, AchievementProvider::Steam, &list).unwrap();
        assert_eq!(newly.len(), 1);
        assert_eq!(newly[0].data.api_name, "B");

        let all = db.load_achievements(id).unwrap();
        assert_eq!(all.len(), 2);
        let a = all.iter().find(|a| a.data.api_name == "A").unwrap();
        assert_eq!(a.icon_path.as_deref(), Some(Path::new("/cache/a.jpg")), "icône conservée");
        assert_eq!(db.sync_status(id, AchievementProvider::Steam).unwrap().unwrap().1, SyncStatus::Ok);
    }

    #[test]
    fn custom_achievements_and_stats() {
        let (mut db, id) = setup();
        let mut a = data("A", true, Some(1_700_000_000));
        a.rarity = Some(3.5);
        db.merge_achievements(id, AchievementProvider::Steam, &[a, data("B", true, Some(1_700_100_000))]).unwrap();
        db.add_custom_achievement(id, "Finir sans mourir", "").unwrap();
        assert!(db.add_custom_achievement(id, "  ", "").is_err());

        let custom = db.load_achievements(id).unwrap().into_iter().find(|a| a.provider == AchievementProvider::Custom).unwrap();
        db.set_custom_unlocked(custom.id, true).unwrap();
        let s = db.achievement_stats().unwrap();
        assert_eq!((s.unlocked, s.total), (3, 3));
        assert_eq!(s.completed_games, vec![id]);
        assert_eq!(s.by_provider.len(), 2);
        assert_eq!(db.rarest_unlocks(5).unwrap()[0].data.api_name, "A");
        assert_eq!(db.recent_unlocks(1).unwrap().len(), 1);
        assert!(!s.per_month.is_empty());

        db.delete_custom_achievement(custom.id).unwrap();
        assert_eq!(db.load_achievements(id).unwrap().len(), 2);
    }
}

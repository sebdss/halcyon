//! Historique des performances (Performance Center) : un résumé par partie.

use crate::{Database, Result};
use halcyon_core::GameId;
use rusqlite::params;

/// Résumé mesuré d'une partie (système entier : processeur, mémoire, carte graphique).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PerfSession {
    pub game_id: GameId,
    pub started_at: i64,
    pub duration_secs: u64,
    pub samples: u32,
    pub cpu_avg: f32,
    pub cpu_max: f32,
    pub ram_avg_mb: u64,
    pub ram_max_mb: u64,
    pub ram_total_mb: u64,
    /// Absent si la carte graphique n'expose pas son taux d'utilisation.
    pub gpu_avg: Option<f32>,
    pub gpu_max: Option<f32>,
    pub vram_max_mb: Option<u64>,
}

/// Parties conservées par jeu (les plus anciennes sont purgées).
const KEEP_PER_GAME: i64 = 50;

impl Database {
    pub fn record_perf_session(&self, p: &PerfSession) -> Result<()> {
        self.conn.execute(
            "INSERT INTO perf_sessions (game_id, started_at, duration_secs, samples, cpu_avg, cpu_max, ram_avg_mb, ram_max_mb, ram_total_mb, gpu_avg, gpu_max, vram_max_mb)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                p.game_id,
                p.started_at,
                p.duration_secs as i64,
                p.samples,
                p.cpu_avg,
                p.cpu_max,
                p.ram_avg_mb as i64,
                p.ram_max_mb as i64,
                p.ram_total_mb as i64,
                p.gpu_avg,
                p.gpu_max,
                p.vram_max_mb.map(|v| v as i64)
            ],
        )?;
        self.conn.execute(
            "DELETE FROM perf_sessions WHERE game_id = ?1 AND id NOT IN (SELECT id FROM perf_sessions WHERE game_id = ?1 ORDER BY started_at DESC LIMIT ?2)",
            params![p.game_id, KEEP_PER_GAME],
        )?;
        Ok(())
    }

    /// Dernières parties mesurées, toutes ou pour un jeu, les plus récentes en premier.
    pub fn perf_sessions(&self, game: Option<GameId>, limit: usize) -> Result<Vec<PerfSession>> {
        let mut stmt = self.conn.prepare(
            "SELECT game_id, started_at, duration_secs, samples, cpu_avg, cpu_max, ram_avg_mb, ram_max_mb, ram_total_mb, gpu_avg, gpu_max, vram_max_mb
             FROM perf_sessions WHERE (?1 IS NULL OR game_id = ?1) ORDER BY started_at DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![game, limit as i64], |r| {
            Ok(PerfSession {
                game_id: r.get(0)?,
                started_at: r.get(1)?,
                duration_secs: r.get::<_, i64>(2)?.max(0) as u64,
                samples: r.get(3)?,
                cpu_avg: r.get(4)?,
                cpu_max: r.get(5)?,
                ram_avg_mb: r.get::<_, i64>(6)?.max(0) as u64,
                ram_max_mb: r.get::<_, i64>(7)?.max(0) as u64,
                ram_total_mb: r.get::<_, i64>(8)?.max(0) as u64,
                gpu_avg: r.get(9)?,
                gpu_max: r.get(10)?,
                vram_max_mb: r.get::<_, Option<i64>>(11)?.map(|v| v.max(0) as u64),
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use halcyon_core::{DiscoveredGame, GameSource, ScanReport};

    #[test]
    fn records_lists_and_purges() {
        let mut db = Database::open_in_memory().unwrap();
        db.merge_scan(GameSource::Steam, &ScanReport { games: vec![DiscoveredGame::new("1", "A")], warnings: vec![], complete: true }).unwrap();
        let id = db.load_games().unwrap()[0].id;
        for i in 0..(KEEP_PER_GAME + 5) {
            let p = PerfSession { game_id: id, started_at: i, duration_secs: 600, samples: 60, cpu_avg: 40.0, cpu_max: 80.0, ram_avg_mb: 8000, ram_max_mb: 9000, ram_total_mb: 16000, gpu_avg: Some(70.0), gpu_max: Some(99.0), vram_max_mb: None };
            db.record_perf_session(&p).unwrap();
        }
        let all = db.perf_sessions(None, 500).unwrap();
        assert_eq!(all.len() as i64, KEEP_PER_GAME);
        assert_eq!(all[0].started_at, KEEP_PER_GAME + 4, "les plus récentes d'abord");
        assert_eq!(all[0].gpu_avg, Some(70.0));
        assert_eq!(db.perf_sessions(Some(id + 1), 10).unwrap().len(), 0);
    }
}

//! Index LaunchBox (1.0.0-alpha.7) : fichier SQLite à part (`launchbox.sqlite` dans le cache),
//! construit une fois depuis `Metadata.zip`, puis seulement lu. Il ne garde que ce qui sert :
//! les noms (normalisés) par plateforme et, pour chaque jeu, son meilleur dos et sa meilleure
//! tranche. Rien n'est chargé en mémoire : chaque recherche est une requête indexée.

use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

/// Version du format : un index plus ancien est reconstruit.
pub const VERSION: i64 = 1;

/// Dos et tranche trouvés pour un jeu (noms de fichiers sur le serveur d'images).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LbArt {
    pub back: Option<String>,
    pub spine: Option<String>,
}

pub struct LbIndex {
    conn: Connection,
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS meta (k TEXT PRIMARY KEY, v TEXT NOT NULL) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS names (key TEXT NOT NULL, platform TEXT NOT NULL, id INTEGER NOT NULL, PRIMARY KEY (key, platform, id)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS images (id INTEGER NOT NULL, part INTEGER NOT NULL, rank INTEGER NOT NULL, file TEXT NOT NULL, PRIMARY KEY (id, part)) WITHOUT ROWID;
";

impl LbIndex {
    /// Ouvre un index existant et complet (`None` s'il manque, est d'une autre version ou n'a
    /// pas été terminé).
    pub fn open(path: &Path) -> Option<Self> {
        if !path.is_file() {
            return None;
        }
        let conn = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
        let v: Option<String> = conn.query_row("SELECT v FROM meta WHERE k = 'version'", [], |r| r.get(0)).optional().ok()?;
        (v.as_deref() == Some(&VERSION.to_string())).then_some(Self { conn })
    }

    /// Date de construction (horodatage Unix).
    pub fn built_at(&self) -> i64 {
        self.conn.query_row("SELECT v FROM meta WHERE k = 'built_at'", [], |r| r.get::<_, String>(0)).ok().and_then(|v| v.parse().ok()).unwrap_or(0)
    }

    /// Nombre de jeux ayant au moins un dos ou une tranche.
    pub fn games_with_art(&self) -> i64 {
        self.conn.query_row("SELECT COUNT(DISTINCT id) FROM images", [], |r| r.get(0)).unwrap_or(0)
    }

    /// Dos et tranche d'un jeu (`key` : titre normalisé). Plusieurs jeux de la base peuvent
    /// porter le même nom : celui qui a le plus de parties, puis les meilleurs rangs, l'emporte.
    pub fn lookup(&self, key: &str, platform: &str) -> crate::Result<LbArt> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT i.id, i.part, i.rank, i.file FROM names n JOIN images i ON i.id = n.id WHERE n.key = ?1 AND n.platform = ?2",
        )?;
        let rows: Vec<(i64, i64, i64, String)> = stmt
            .query_map(params![key, platform], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let mut best: Option<(i64, usize, i64)> = None;
        let mut ids: Vec<i64> = rows.iter().map(|r| r.0).collect();
        ids.sort_unstable();
        ids.dedup();
        for id in ids {
            let parts: Vec<&(i64, i64, i64, String)> = rows.iter().filter(|r| r.0 == id).collect();
            let score = (parts.len(), parts.iter().map(|r| r.2).sum::<i64>());
            if best.is_none_or(|(_, n, s)| score.0 > n || (score.0 == n && score.1 < s)) {
                best = Some((id, score.0, score.1));
            }
        }
        let Some((id, _, _)) = best else { return Ok(LbArt::default()) };
        let file = |part: i64| rows.iter().find(|r| r.0 == id && r.1 == part).map(|r| r.3.clone());
        Ok(LbArt { back: file(1), spine: file(2) })
    }
}

/// Construction d'un index : dans un fichier temporaire, renommé seulement une fois complet
/// (un index interrompu n'est jamais lu).
pub struct LbWriter {
    conn: Connection,
}

impl LbWriter {
    pub fn create(tmp: &Path) -> crate::Result<Self> {
        let _ = std::fs::remove_file(tmp);
        let conn = Connection::open(tmp)?;
        conn.execute_batch("PRAGMA journal_mode = OFF; PRAGMA synchronous = OFF;")?;
        conn.execute_batch(SCHEMA)?;
        conn.execute_batch("BEGIN")?;
        Ok(Self { conn })
    }

    pub fn add_name(&self, id: i64, key: &str, platform: &str) -> crate::Result<()> {
        if key.is_empty() {
            return Ok(());
        }
        self.conn.prepare_cached("INSERT OR IGNORE INTO names (key, platform, id) VALUES (?1, ?2, ?3)")?.execute(params![key, platform, id])?;
        Ok(())
    }

    /// Garde la meilleure image (rang le plus petit) par jeu et par partie.
    pub fn add_image(&self, id: i64, part: i64, rank: i64, file: &str) -> crate::Result<()> {
        self.conn
            .prepare_cached(
                "INSERT INTO images (id, part, rank, file) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (id, part) DO UPDATE SET rank = excluded.rank, file = excluded.file WHERE excluded.rank < images.rank",
            )?
            .execute(params![id, part, rank, file])?;
        Ok(())
    }

    /// Termine : supprime les images des jeux non retenus (autres plateformes) et les noms sans
    /// image, écrit la version, puis renomme.
    pub fn finish(self, tmp: &Path, dest: &Path, built_at: i64) -> crate::Result<()> {
        self.conn.execute_batch("DELETE FROM images WHERE id NOT IN (SELECT id FROM names); DELETE FROM names WHERE id NOT IN (SELECT id FROM images);")?;
        self.conn.execute("INSERT OR REPLACE INTO meta (k, v) VALUES ('built_at', ?1)", params![built_at.to_string()])?;
        self.conn.execute("INSERT OR REPLACE INTO meta (k, v) VALUES ('version', ?1)", params![VERSION.to_string()])?;
        self.conn.execute_batch("COMMIT; VACUUM;")?;
        drop(self.conn);
        std::fs::rename(tmp, dest)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_then_lookup() {
        let dir = tempfile::tempdir().unwrap();
        let (tmp, dest) = (dir.path().join("lb.tmp"), dir.path().join("lb.sqlite"));
        assert!(LbIndex::open(&dest).is_none());
        let w = LbWriter::create(&tmp).unwrap();
        w.add_name(1, "half life 2", "Windows").unwrap();
        w.add_name(2, "half life 2", "Windows").unwrap();
        w.add_name(3, "no art", "Windows").unwrap();
        w.add_image(1, 1, 6, "jp-back.jpg").unwrap();
        w.add_image(2, 1, 3, "eu-back.jpg").unwrap();
        w.add_image(2, 1, 1, "us-back.jpg").unwrap();
        w.add_image(2, 1, 5, "worse.jpg").unwrap();
        w.add_image(2, 2, 0, "spine.png").unwrap();
        w.finish(&tmp, &dest, 42).unwrap();
        assert!(!tmp.exists());
        let idx = LbIndex::open(&dest).unwrap();
        assert_eq!(idx.built_at(), 42);
        assert_eq!(idx.games_with_art(), 2);
        // Le jeu qui a dos et tranche l'emporte, avec sa meilleure image.
        assert_eq!(idx.lookup("half life 2", "Windows").unwrap(), LbArt { back: Some("us-back.jpg".into()), spine: Some("spine.png".into()) });
        assert_eq!(idx.lookup("half life 2", "Sony Playstation").unwrap(), LbArt::default());
        assert_eq!(idx.lookup("no art", "Windows").unwrap(), LbArt::default());
    }

    #[test]
    fn unfinished_index_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let tmp = dir.path().join("lb.tmp");
        let w = LbWriter::create(&tmp).unwrap();
        w.add_name(1, "x", "Windows").unwrap();
        drop(w);
        assert!(LbIndex::open(&tmp).is_none());
    }
}

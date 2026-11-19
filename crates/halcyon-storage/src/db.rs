//! Ouverture de la base SQLite et application des migrations.

use crate::migrations::MIGRATIONS;
use rusqlite::Connection;
use std::path::Path;

/// Accès à la base. Une seule connexion, protégée par un `Mutex` côté appelant :
/// les requêtes sont courtes et SQLite en mode WAL sérialise de toute façon les écritures.
pub struct Database {
    pub(crate) conn: Connection,
}

impl Database {
    /// Vérification d'intégrité SQLite : `None` si la base est saine, sinon les problèmes signalés.
    pub fn integrity_check(&self) -> crate::Result<Option<String>> {
        let mut stmt = self.conn.prepare("PRAGMA integrity_check")?;
        let rows: Vec<String> = stmt.query_map([], |r| r.get::<_, String>(0))?.collect::<rusqlite::Result<_>>()?;
        Ok(if rows.len() == 1 && rows[0] == "ok" { None } else { Some(rows.join(" ; ")) })
    }

    pub fn open(path: &Path) -> crate::Result<Self> {
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> crate::Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> crate::Result<Self> {
        // WAL : lectures non bloquées par les écritures, et base cohérente
        // même après une fermeture brutale. synchronous=NORMAL suffit en WAL.
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 3000;",
        )?;
        let mut db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&mut self) -> crate::Result<()> {
        let current: usize = self.conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))? as usize;
        if current > MIGRATIONS.len() {
            log::warn!("base créée par une version plus récente (schéma v{current})");
            return Ok(());
        }
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(current) {
            let version = i + 1;
            let tx = self.conn.transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", version as i64)?;
            tx.commit()?;
            log::info!("migration du schéma vers v{version}");
        }
        Ok(())
    }

    pub fn schema_version(&self) -> crate::Result<i64> {
        Ok(self.conn.query_row("PRAGMA user_version", [], |r| r.get(0))?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Base 0.3 (schéma v4) avec des données : la migration v5 n'ajoute qu'un index et
    /// conserve tout.
    #[test]
    fn upgrade_from_v4_keeps_data_and_adds_index() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.db");
        {
            let mut conn = rusqlite::Connection::open(&path).unwrap();
            for (i, sql) in MIGRATIONS.iter().take(4).enumerate() {
                let tx = conn.transaction().unwrap();
                tx.execute_batch(sql).unwrap();
                tx.pragma_update(None, "user_version", (i + 1) as i64).unwrap();
                tx.commit().unwrap();
            }
            conn.execute(
                "INSERT INTO games (id, source, source_key, name, added_at) VALUES (1, 'manual', 'x', 'Jeu de test', 1)",
                [],
            )
            .unwrap();
            conn.execute("INSERT INTO play_sessions (game_id, started_at, duration_secs) VALUES (1, 1000, 600)", []).unwrap();
            conn.execute("INSERT INTO play_sessions (game_id, started_at, duration_secs) VALUES (1, 5000, 60)", []).unwrap();
        }
        let db = Database::open(&path).unwrap();
        assert_eq!(db.schema_version().unwrap(), MIGRATIONS.len() as i64);
        let has_index: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'idx_sessions_started'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(has_index, 1);
        assert_eq!(db.sessions().unwrap().len(), 2);
        let recent = db.sessions_since(2000).unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].duration_secs, 60);
        // Réouverture : rien n'est rejoué.
        drop(db);
        let db = Database::open(&path).unwrap();
        assert_eq!(db.schema_version().unwrap(), MIGRATIONS.len() as i64);
    }

    /// Base 1.0.0-alpha.5 (schéma v6) avec des collections : la migration v7 ajoute style et
    /// positions sans rien perdre ; l'ordre manuel et les ajouts suivants fonctionnent.
    #[test]
    fn upgrade_from_v6_keeps_collections() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.db");
        {
            let mut conn = rusqlite::Connection::open(&path).unwrap();
            for (i, sql) in MIGRATIONS.iter().take(6).enumerate() {
                let tx = conn.transaction().unwrap();
                tx.execute_batch(sql).unwrap();
                tx.pragma_update(None, "user_version", (i + 1) as i64).unwrap();
                tx.commit().unwrap();
            }
            for (id, name) in [(1, "Sonic Mania"), (2, "Sonic Frontiers"), (3, "Celeste")] {
                conn.execute(
                    "INSERT INTO games (id, source, source_key, name, added_at) VALUES (?1, 'manual', ?2, ?2, 1)",
                    rusqlite::params![id, name],
                )
                .unwrap();
            }
            conn.execute("INSERT INTO collections (id, name) VALUES (7, 'Sonic')", []).unwrap();
            conn.execute("INSERT INTO collection_games (collection_id, game_id) VALUES (7, 1), (7, 2)", []).unwrap();
        }
        let mut db = Database::open(&path).unwrap();
        assert_eq!(db.schema_version().unwrap(), MIGRATIONS.len() as i64);
        let cols = db.collections().unwrap();
        assert_eq!(cols.len(), 1);
        assert_eq!((cols[0].id, cols[0].name.as_str(), cols[0].count, cols[0].color.clone(), cols[0].position), (7, "Sonic", 2, None, 0));
        let mut pos = db.collection_positions(7).unwrap();
        pos.sort();
        assert_eq!(pos, vec![(1, 0), (2, 0)], "jeux repris, sans ordre manuel");

        // Ordre manuel, puis ajout : le nouveau jeu va à la fin.
        db.set_collection_order(7, &[2, 1]).unwrap();
        db.set_in_collection(7, 3, true).unwrap();
        let mut pos = db.collection_positions(7).unwrap();
        pos.sort_by_key(|p| p.1);
        assert_eq!(pos, vec![(2, 1), (1, 2), (3, 3)]);

        db.set_collection_style(7, Some("#3d7fc4"), Some(Path::new("/tmp/logo.png")), None).unwrap();
        db.rename_collection(7, "Sonic the Hedgehog").unwrap();
        let c = &db.collections().unwrap()[0];
        assert_eq!((c.name.as_str(), c.color.as_deref(), c.logo.as_deref()), ("Sonic the Hedgehog", Some("#3d7fc4"), Some(Path::new("/tmp/logo.png"))));
        let other = db.create_collection_with("Indés", &[3]).unwrap();
        assert!(db.rename_collection(other, "sonic the hedgehog").is_err(), "nom déjà pris");
        assert_eq!(db.add_to_collection(other, &[1, 3]).unwrap(), 1, "doublon ignoré");
        db.set_collections_order(&[other, 7]).unwrap();
        assert_eq!(db.collections().unwrap().iter().map(|c| c.id).collect::<Vec<_>>(), vec![other, 7]);
        assert_eq!(db.games_in_collections().unwrap().len(), 3);

        // Désinstallation : le jeu reste, ses collections aussi.
        db.set_installed(1, false).unwrap();
        let g = db.load_games().unwrap().into_iter().find(|g| g.id == 1).unwrap();
        assert!(!g.installed);
        assert!(db.game_collections(1).unwrap().contains(&7));
    }
}

//! Configuration des sauvegardes et des mods par jeu, et export de la base.

use crate::{Database, Result, StorageError};
use halcyon_core::GameId;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Réglages de sauvegarde d'un jeu.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SaveConfig {
    /// Emplacements (chemins à jetons, voir `halcyon_integrations::saves`).
    pub locations: Vec<String>,
    pub auto_before: bool,
    pub auto_after: bool,
    /// Dernier instantané créé ou restauré sur ce PC, et son empreinte.
    pub last_id: Option<String>,
    pub last_hash: Option<String>,
    pub last_at: Option<i64>,
}

/// Un mod dans un profil de mods (ordre de chargement = ordre de la liste).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModEntry {
    pub mod_id: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModProfile {
    pub id: i64,
    pub game_id: GameId,
    pub name: String,
    pub entries: Vec<ModEntry>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModState {
    /// Adaptateur (JSON de `halcyon_integrations::mods::Adapter`).
    pub adapter: Option<String>,
    pub active_profile: Option<i64>,
    pub nexus_domain: Option<String>,
}

impl Database {
    pub fn save_config(&self, game: GameId) -> Result<Option<SaveConfig>> {
        Ok(self
            .conn
            .query_row(
                "SELECT locations, auto_before, auto_after, last_id, last_hash, last_at FROM save_config WHERE game_id = ?1",
                [game],
                |r| {
                    Ok(SaveConfig {
                        locations: serde_json::from_str(&r.get::<_, String>(0)?).unwrap_or_default(),
                        auto_before: r.get(1)?,
                        auto_after: r.get(2)?,
                        last_id: r.get(3)?,
                        last_hash: r.get(4)?,
                        last_at: r.get(5)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn set_save_config(&self, game: GameId, c: &SaveConfig) -> Result<()> {
        self.conn.execute(
            "INSERT INTO save_config (game_id, locations, auto_before, auto_after, last_id, last_hash, last_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT (game_id) DO UPDATE SET locations = excluded.locations, auto_before = excluded.auto_before,
                auto_after = excluded.auto_after, last_id = excluded.last_id, last_hash = excluded.last_hash,
                last_at = excluded.last_at",
            params![game, serde_json::to_string(&c.locations)?, c.auto_before, c.auto_after, c.last_id, c.last_hash, c.last_at],
        )?;
        Ok(())
    }

    /// Jeux ayant au moins un emplacement de sauvegarde configuré.
    pub fn save_configs(&self) -> Result<Vec<(GameId, SaveConfig)>> {
        let mut stmt = self.conn.prepare("SELECT game_id FROM save_config WHERE locations != '[]'")?;
        let ids: Vec<GameId> = stmt.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
        let mut out = Vec::new();
        for id in ids {
            if let Some(c) = self.save_config(id)? {
                out.push((id, c));
            }
        }
        Ok(out)
    }

    pub fn mod_state(&self, game: GameId) -> Result<ModState> {
        Ok(self
            .conn
            .query_row("SELECT adapter, active_profile, nexus_domain FROM mod_state WHERE game_id = ?1", [game], |r| {
                Ok(ModState { adapter: r.get(0)?, active_profile: r.get(1)?, nexus_domain: r.get(2)? })
            })
            .optional()?
            .unwrap_or_default())
    }

    pub fn set_mod_state(&self, game: GameId, s: &ModState) -> Result<()> {
        self.conn.execute(
            "INSERT INTO mod_state (game_id, adapter, active_profile, nexus_domain) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (game_id) DO UPDATE SET adapter = excluded.adapter, active_profile = excluded.active_profile,
                nexus_domain = excluded.nexus_domain",
            params![game, s.adapter, s.active_profile, s.nexus_domain],
        )?;
        Ok(())
    }

    pub fn mod_profiles(&self, game: GameId) -> Result<Vec<ModProfile>> {
        let mut stmt = self.conn.prepare("SELECT id, name, entries FROM mod_profiles WHERE game_id = ?1 ORDER BY id")?;
        let rows = stmt.query_map([game], |r| {
            Ok(ModProfile {
                id: r.get(0)?,
                game_id: game,
                name: r.get(1)?,
                entries: serde_json::from_str(&r.get::<_, String>(2)?).unwrap_or_default(),
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn save_mod_profile(&self, p: &ModProfile) -> Result<i64> {
        let name = p.name.trim();
        if name.is_empty() {
            return Err(StorageError::Conflict(tr!("le profil de mods doit avoir un nom").into()));
        }
        let entries = serde_json::to_string(&p.entries)?;
        if p.id == 0 {
            self.conn.execute(
                "INSERT INTO mod_profiles (game_id, name, entries, updated_at) VALUES (?1, ?2, ?3, ?4)",
                params![p.game_id, name, entries, crate::now_unix()],
            )?;
            Ok(self.conn.last_insert_rowid())
        } else {
            self.conn.execute(
                "UPDATE mod_profiles SET name = ?2, entries = ?3, updated_at = ?4 WHERE id = ?1",
                params![p.id, name, entries, crate::now_unix()],
            )?;
            Ok(p.id)
        }
    }

    pub fn delete_mod_profile(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM mod_profiles WHERE id = ?1", [id])?;
        self.conn.execute("UPDATE mod_state SET active_profile = NULL WHERE active_profile = ?1", [id])?;
        Ok(())
    }

    /// Copie cohérente de la base (même pendant son utilisation), pour l'export.
    pub fn export_copy(&self, dest: &Path) -> Result<()> {
        if dest.exists() {
            std::fs::remove_file(dest)?;
        }
        self.conn.execute("VACUUM INTO ?1", [dest.to_string_lossy()])?;
        Ok(())
    }
}

/// Vérifie qu'un fichier est une base Halcyon lisible et renvoie sa version de schéma.
pub fn inspect_database(path: &Path) -> Result<i64> {
    let conn = rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    let games: i64 = conn.query_row("SELECT COUNT(*) FROM games", [], |r| r.get(0))?;
    let _ = games;
    let ok: String = conn.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
    if ok != "ok" {
        return Err(StorageError::Conflict(tr!("base endommagée : {ok}", ok = ok)));
    }
    Ok(version)
}

/// Version de schéma gérée par cette version de Halcyon.
pub fn schema_version() -> i64 {
    crate::migrations::MIGRATIONS.len() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use halcyon_core::{DiscoveredGame, GameSource, ScanReport};

    #[test]
    fn configs_profiles_and_export() {
        let mut db = Database::open_in_memory().unwrap();
        db.merge_scan(GameSource::Steam, &ScanReport { games: vec![DiscoveredGame::new("1", "G")], warnings: vec![], complete: true })
            .unwrap();
        let id = db.load_games().unwrap()[0].id;
        assert!(db.save_config(id).unwrap().is_none());
        let c = SaveConfig { locations: vec!["{documents}/G".into()], auto_before: true, auto_after: false, ..Default::default() };
        db.set_save_config(id, &c).unwrap();
        assert_eq!(db.save_config(id).unwrap(), Some(c));
        assert_eq!(db.save_configs().unwrap().len(), 1);

        let p = ModProfile { id: 0, game_id: id, name: "Vanilla+".into(), entries: vec![ModEntry { mod_id: "a".into(), enabled: true }] };
        let pid = db.save_mod_profile(&p).unwrap();
        db.set_mod_state(id, &ModState { active_profile: Some(pid), ..Default::default() }).unwrap();
        assert_eq!(db.mod_profiles(id).unwrap()[0].entries.len(), 1);
        db.delete_mod_profile(pid).unwrap();
        assert_eq!(db.mod_state(id).unwrap().active_profile, None);

        let tmp = tempfile::tempdir().unwrap();
        let out = tmp.path().join("copy.db");
        db.export_copy(&out).unwrap();
        assert_eq!(inspect_database(&out).unwrap(), schema_version());
    }
}

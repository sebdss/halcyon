//! Accès à la bibliothèque : lecture, fusion des scans, actions utilisateur.

use crate::activity::{log_with, ActivityKind};
use crate::{now_unix, Database, Result, StorageError};
use halcyon_core::model::overrides;
use halcyon_core::util::path_key;
use halcyon_core::{
    Artwork, Collection, Game, GameId, GameSource, LaunchSettingsUpdate, ManualGameInput, MetadataUpdate, OnlineMetadata,
    PlayStatus, ScanReport, UpdateInfo, UpdateState,
};
use rusqlite::{params, OptionalExtension, Row};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Bilan d'une fusion de scan.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MergeStats {
    pub added: usize,
    pub updated: usize,
    pub uninstalled: usize,
    /// Jeux pour lesquels une mise à jour vient d'être signalée (id, nom).
    pub new_updates: Vec<(GameId, String)>,
}

struct Existing {
    id: GameId,
    installed: bool,
    name: String,
    version: Option<String>,
    update: UpdateState,
}

/// Au-delà, les ajouts d'un scan sont résumés en une seule entrée du journal.
const ACTIVITY_DETAIL_LIMIT: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArtworkKind {
    Cover,
    Banner,
    Logo,
}

impl ArtworkKind {
    fn column(self) -> &'static str {
        match self {
            Self::Cover => "cover_path",
            Self::Banner => "banner_path",
            Self::Logo => "logo_path",
        }
    }

    fn override_flag(self) -> u32 {
        match self {
            Self::Cover => overrides::COVER,
            Self::Banner => overrides::BANNER,
            Self::Logo => 0,
        }
    }
}

const COLUMNS: &str = "id, source, source_key, name, description, developer, publisher, release_date, genres,
    install_dir, executable, arguments, working_dir, launch_uri, installed, favorite, hidden,
    cover_path, banner_path, logo_path, cover_url, banner_url, logo_url,
    added_at, last_played, playtime_secs, play_count, size_bytes, overrides,
    platform, emulator, ra_game_id, screenshots,
    status, completed_at, notes, playtime_platform_secs, playtime_manual_secs, tags, version,
    update_state, update_downloaded, update_total, installed_at";

fn opt_path(p: &Option<PathBuf>) -> Option<String> {
    p.as_ref().map(|p| p.to_string_lossy().into_owned())
}

fn row_to_game(r: &Row) -> rusqlite::Result<Game> {
    let source_str: String = r.get(1)?;
    let genres: String = r.get(8)?;
    let path = |i: usize| -> rusqlite::Result<Option<PathBuf>> { Ok(r.get::<_, Option<String>>(i)?.map(PathBuf::from)) };
    let mut g = Game {
        id: r.get(0)?,
        source: GameSource::parse(&source_str).unwrap_or(GameSource::Manual),
        source_key: r.get(2)?,
        name: r.get(3)?,
        description: r.get(4)?,
        developer: r.get(5)?,
        publisher: r.get(6)?,
        release_date: r.get(7)?,
        genres: serde_json::from_str(&genres).unwrap_or_default(),
        install_dir: path(9)?,
        executable: path(10)?,
        arguments: r.get(11)?,
        working_dir: path(12)?,
        launch_uri: r.get(13)?,
        installed: r.get(14)?,
        favorite: r.get(15)?,
        hidden: r.get(16)?,
        artwork: Artwork {
            cover: path(17)?,
            banner: path(18)?,
            logo: path(19)?,
            cover_url: r.get(20)?,
            banner_url: r.get(21)?,
            logo_url: r.get(22)?,
        },
        added_at: r.get(23)?,
        last_played: r.get(24)?,
        playtime_secs: r.get::<_, i64>(25)?.max(0) as u64,
        play_count: r.get::<_, i64>(26)?.max(0) as u32,
        size_bytes: r.get::<_, Option<i64>>(27)?.map(|v| v.max(0) as u64),
        overrides: r.get::<_, i64>(28)? as u32,
        platform: r.get(29)?,
        emulator: r.get(30)?,
        ra_game_id: r.get::<_, Option<i64>>(31)?.map(|v| v as u32),
        screenshots: serde_json::from_str(&r.get::<_, String>(32)?).unwrap_or_default(),
        status: PlayStatus::parse(&r.get::<_, String>(33)?),
        completed_at: r.get(34)?,
        notes: r.get(35)?,
        playtime_platform_secs: r.get::<_, Option<i64>>(36)?.map(|v| v.max(0) as u64),
        playtime_manual_secs: r.get::<_, Option<i64>>(37)?.map(|v| v.max(0) as u64),
        tags: serde_json::from_str(&r.get::<_, String>(38)?).unwrap_or_default(),
        version: r.get(39)?,
        update: UpdateInfo {
            state: UpdateState::parse(&r.get::<_, String>(40)?),
            downloaded: r.get::<_, i64>(41)?.max(0) as u64,
            total: r.get::<_, i64>(42)?.max(0) as u64,
        },
        installed_at: r.get(43)?,
        sort_key: String::new(),
        search_key: String::new(),
    };
    g.refresh_derived();
    Ok(g)
}

impl Database {
    /// Charge toute la bibliothèque (y compris les jeux masqués et désinstallés).
    pub fn load_games(&self) -> Result<Vec<Game>> {
        let mut stmt = self.conn.prepare(&format!("SELECT {COLUMNS} FROM games"))?;
        let games = stmt.query_map([], row_to_game)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(games)
    }

    pub fn get_game(&self, id: GameId) -> Result<Game> {
        self.conn
            .query_row(&format!("SELECT {COLUMNS} FROM games WHERE id = ?1"), [id], row_to_game)
            .optional()?
            .ok_or(StorageError::NotFound(id))
    }

    /// Fusionne le résultat d'un scan dans la bibliothèque, en une transaction.
    ///
    /// - nouveaux jeux insérés ;
    /// - jeux existants mis à jour sans écraser les champs personnalisés ;
    /// - temps de jeu et dernier lancement : on garde le maximum (plateforme vs Halcyon) ;
    /// - si le scan est complet, les jeux absents sont marqués désinstallés (jamais supprimés).
    pub fn merge_scan(&mut self, source: GameSource, report: &ScanReport) -> Result<MergeStats> {
        let now = now_unix();
        let tx = self.conn.transaction()?;
        let mut stats = MergeStats::default();

        let mut existing: HashMap<String, Existing> = HashMap::new();
        {
            let mut stmt =
                tx.prepare("SELECT source_key, id, installed, name, version, update_state FROM games WHERE source = ?1")?;
            let rows = stmt.query_map([source.as_str()], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    Existing {
                        id: r.get(1)?,
                        installed: r.get(2)?,
                        name: r.get(3)?,
                        version: r.get(4)?,
                        update: UpdateState::parse(&r.get::<_, String>(5)?),
                    },
                ))
            })?;
            for row in rows {
                let (k, e) = row?;
                existing.insert(k, e);
            }
        }
        let mut added_names: Vec<String> = Vec::new();
        // Un exécutable déjà ajouté à la main n'est pas réimporté comme standalone.
        let mut reserved: HashSet<String> = HashSet::new();
        if source == GameSource::Standalone {
            let mut stmt = tx.prepare("SELECT source_key FROM games WHERE source = 'manual'")?;
            for k in stmt.query_map([], |r| r.get::<_, String>(0))? {
                reserved.insert(k?);
            }
        }

        let mut seen: HashSet<&str> = HashSet::new();
        {
            let mut update = tx.prepare(
                "UPDATE games SET
                    name         = CASE WHEN overrides & ?2 THEN name ELSE ?3 END,
                    description  = CASE WHEN overrides & ?4 THEN description ELSE COALESCE(?5, description) END,
                    developer    = CASE WHEN overrides & 32 THEN developer ELSE COALESCE(?6, developer) END,
                    publisher    = CASE WHEN overrides & 32 THEN publisher ELSE COALESCE(?7, publisher) END,
                    release_date = CASE WHEN overrides & 32 THEN release_date ELSE COALESCE(?8, release_date) END,
                    genres       = CASE WHEN ?9 = '[]' OR overrides & 32 THEN genres ELSE ?9 END,
                    install_dir  = ?10,
                    executable   = CASE WHEN overrides & ?11 THEN executable ELSE ?12 END,
                    arguments    = CASE WHEN overrides & ?11 THEN arguments ELSE ?13 END,
                    working_dir  = CASE WHEN overrides & ?11 THEN working_dir ELSE ?14 END,
                    launch_uri   = ?15,
                    installed    = ?16,
                    cover_path   = CASE WHEN overrides & ?17 THEN cover_path ELSE COALESCE(?18, cover_path) END,
                    banner_path  = CASE WHEN overrides & ?19 THEN banner_path ELSE COALESCE(?20, banner_path) END,
                    logo_path    = COALESCE(?21, logo_path),
                    cover_url    = COALESCE(?22, cover_url),
                    banner_url   = COALESCE(?23, banner_url),
                    logo_url     = COALESCE(?24, logo_url),
                    last_played  = NULLIF(MAX(COALESCE(last_played, 0), COALESCE(?25, 0)), 0),
                    playtime_secs = MAX(playtime_secs, COALESCE(?26, 0)),
                    size_bytes   = COALESCE(?27, size_bytes),
                    platform     = COALESCE(?28, platform),
                    playtime_platform_secs = COALESCE(?26, playtime_platform_secs),
                    version      = COALESCE(?29, version),
                    update_state = COALESCE(?30, update_state),
                    update_downloaded = COALESCE(?31, update_downloaded),
                    update_total = COALESCE(?32, update_total),
                    installed_at = CASE WHEN ?16 AND (installed = 0 OR installed_at IS NULL) THEN ?33
                                        ELSE installed_at END
                 WHERE id = ?1",
            )?;
            let mut insert = tx.prepare(
                "INSERT INTO games (source, source_key, name, description, developer, publisher, release_date,
                    genres, install_dir, executable, arguments, working_dir, launch_uri, installed,
                    cover_path, banner_path, logo_path, cover_url, banner_url, logo_url,
                    added_at, last_played, playtime_secs, size_bytes, platform,
                    playtime_platform_secs, version, update_state, update_downloaded, update_total, installed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20,
                    ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, CASE WHEN ?14 THEN ?21 END)",
            )?;

            for d in &report.games {
                if d.source_key.is_empty() || reserved.contains(&d.source_key) || !seen.insert(&d.source_key) {
                    continue;
                }
                let genres = serde_json::to_string(&d.genres)?;
                let a = &d.artwork;
                let upd = d.update;
                let upd_state = upd.map(|u| u.state.as_str());
                let upd_done = upd.map(|u| u.downloaded as i64);
                let upd_total = upd.map(|u| u.total as i64);
                if let Some(e) = existing.get(&d.source_key) {
                    let id = e.id;
                    update.execute(params![
                        id,
                        overrides::NAME,
                        d.name,
                        overrides::DESCRIPTION,
                        d.description,
                        d.developer,
                        d.publisher,
                        d.release_date,
                        genres,
                        opt_path(&d.install_dir),
                        overrides::LAUNCH,
                        opt_path(&d.executable),
                        d.arguments,
                        opt_path(&d.working_dir),
                        d.launch_uri,
                        d.installed,
                        overrides::COVER,
                        opt_path(&a.cover),
                        overrides::BANNER,
                        opt_path(&a.banner),
                        opt_path(&a.logo),
                        a.cover_url,
                        a.banner_url,
                        a.logo_url,
                        d.last_played,
                        d.playtime_secs.map(|v| v as i64),
                        d.size_bytes.map(|v| v as i64),
                        d.platform,
                        d.version,
                        upd_state,
                        upd_done,
                        upd_total,
                        now,
                    ])?;
                    stats.updated += 1;
                    // Historique des versions et événements du journal.
                    if let Some(v) = d.version.as_deref().filter(|v| !v.is_empty()) {
                        if e.version.as_deref() != Some(v) {
                            tx.execute(
                                "INSERT INTO version_history (game_id, version, seen_at) VALUES (?1, ?2, ?3)",
                                params![id, v, now],
                            )?;
                            if let Some(old) = &e.version {
                                log_with(&tx, ActivityKind::Update, Some(id), &format!("{} mis à jour", e.name), &format!("{old} → {v}"))?;
                            }
                        }
                    }
                    if let Some(u) = upd {
                        if u.state == UpdateState::Available && e.update == UpdateState::None {
                            log_with(&tx, ActivityKind::Update, Some(id), &format!("Mise à jour disponible pour {}", e.name), source.label())?;
                            stats.new_updates.push((id, e.name.clone()));
                        }
                    }
                    if d.installed && !e.installed {
                        log_with(&tx, ActivityKind::Installed, Some(id), &format!("{} installé", e.name), source.label())?;
                    }
                } else {
                    insert.execute(params![
                        source.as_str(),
                        d.source_key,
                        d.name,
                        d.description,
                        d.developer,
                        d.publisher,
                        d.release_date,
                        genres,
                        opt_path(&d.install_dir),
                        opt_path(&d.executable),
                        d.arguments,
                        opt_path(&d.working_dir),
                        d.launch_uri,
                        d.installed,
                        opt_path(&a.cover),
                        opt_path(&a.banner),
                        opt_path(&a.logo),
                        a.cover_url,
                        a.banner_url,
                        a.logo_url,
                        now,
                        d.last_played,
                        d.playtime_secs.unwrap_or(0) as i64,
                        d.size_bytes.map(|v| v as i64),
                        d.platform,
                        d.playtime_secs.map(|v| v as i64),
                        d.version,
                        upd_state.unwrap_or(""),
                        upd_done.unwrap_or(0),
                        upd_total.unwrap_or(0),
                    ])?;
                    stats.added += 1;
                    let id = tx.last_insert_rowid();
                    if let Some(v) = d.version.as_deref().filter(|v| !v.is_empty()) {
                        tx.execute(
                            "INSERT INTO version_history (game_id, version, seen_at) VALUES (?1, ?2, ?3)",
                            params![id, v, now],
                        )?;
                    }
                    if added_names.len() < ACTIVITY_DETAIL_LIMIT {
                        log_with(&tx, ActivityKind::Added, Some(id), &format!("{} ajouté à la bibliothèque", d.name), source.label())?;
                    }
                    added_names.push(d.name.clone());
                }
            }

            if added_names.len() > ACTIVITY_DETAIL_LIMIT {
                let more = added_names.len() - ACTIVITY_DETAIL_LIMIT;
                log_with(
                    &tx,
                    ActivityKind::Added,
                    None,
                    &format!("{more} autres jeux ajoutés ({})", source.label()),
                    &added_names[ACTIVITY_DETAIL_LIMIT..].iter().take(30).cloned().collect::<Vec<_>>().join(", "),
                )?;
            }
            if report.complete {
                let mut mark = tx.prepare("UPDATE games SET installed = 0, update_state = '' WHERE id = ?1")?;
                for (key, e) in &existing {
                    if e.installed && !seen.contains(key.as_str()) {
                        mark.execute([e.id])?;
                        stats.uninstalled += 1;
                        log_with(&tx, ActivityKind::Uninstalled, Some(e.id), &format!("{} désinstallé", e.name), source.label())?;
                    }
                }
            }
        }
        tx.commit()?;
        Ok(stats)
    }

    /// Ajoute un jeu saisi manuellement. Refuse les doublons (même exécutable).
    pub fn add_manual(&mut self, input: &ManualGameInput) -> Result<Game> {
        let key = path_key(&input.executable);
        let dup: Option<String> = self
            .conn
            .query_row(
                "SELECT name FROM games WHERE source_key = ?1 AND source IN ('manual', 'standalone')",
                [&key],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(name) = dup {
            return Err(StorageError::Conflict(format!("cet exécutable est déjà dans la bibliothèque (« {name} »)")));
        }
        let name = if input.name.trim().is_empty() {
            input.executable.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Jeu".into())
        } else {
            input.name.trim().to_string()
        };
        self.conn.execute(
            "INSERT INTO games (source, source_key, name, install_dir, executable, arguments, working_dir,
                installed, added_at, installed_at, overrides)
             VALUES ('manual', ?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, ?7, ?8)",
            params![
                key,
                name,
                input.executable.parent().map(|p| p.to_string_lossy().into_owned()),
                input.executable.to_string_lossy(),
                input.arguments.trim(),
                opt_path(&input.working_dir),
                now_unix(),
                overrides::NAME | overrides::LAUNCH,
            ],
        )?;
        let id = self.conn.last_insert_rowid();
        self.log_activity(ActivityKind::Added, Some(id), &format!("{name} ajouté à la bibliothèque"), "Ajout manuel")?;
        self.get_game(id)
    }

    pub fn set_favorite(&self, id: GameId, favorite: bool) -> Result<()> {
        self.conn.execute("UPDATE games SET favorite = ?2 WHERE id = ?1", params![id, favorite])?;
        Ok(())
    }

    pub fn set_hidden(&self, id: GameId, hidden: bool) -> Result<()> {
        self.conn.execute("UPDATE games SET hidden = ?2 WHERE id = ?1", params![id, hidden])?;
        Ok(())
    }

    /// Modifie le nom et les paramètres de lancement ; ces champs deviennent prioritaires
    /// sur les synchronisations.
    pub fn update_launch_settings(&self, id: GameId, u: &LaunchSettingsUpdate) -> Result<Game> {
        let current = self.get_game(id)?;
        let mut flags = current.overrides;
        if u.name.trim() != current.name && !u.name.trim().is_empty() {
            flags |= overrides::NAME;
        }
        let launch_changed =
            u.executable != current.executable || u.arguments.trim() != current.arguments || u.working_dir != current.working_dir;
        if launch_changed {
            // Exécutable vidé sur un jeu de plateforme : retour au lancement via le client.
            if u.executable.is_none() && current.source.uses_platform_client() {
                flags &= !overrides::LAUNCH;
            } else {
                flags |= overrides::LAUNCH;
            }
        }
        let name = if u.name.trim().is_empty() { current.name.clone() } else { u.name.trim().to_string() };
        self.conn.execute(
            "UPDATE games SET name = ?2, executable = ?3, arguments = ?4, working_dir = ?5, overrides = ?6 WHERE id = ?1",
            params![id, name, opt_path(&u.executable), u.arguments.trim(), opt_path(&u.working_dir), flags],
        )?;
        self.get_game(id)
    }

    /// Enregistre une session de jeu mesurée (jeux lancés directement).
    pub fn record_session(&self, id: GameId, started_at: i64, duration_secs: u64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO play_sessions (game_id, started_at, duration_secs) VALUES (?1, ?2, ?3)",
            params![id, started_at, duration_secs as i64],
        )?;
        self.conn.execute(
            "UPDATE games SET playtime_secs = playtime_secs + ?2, last_played = MAX(COALESCE(last_played, 0), ?3)
             WHERE id = ?1",
            params![id, duration_secs as i64, started_at],
        )?;
        let name: String = self.conn.query_row("SELECT name FROM games WHERE id = ?1", [id], |r| r.get(0))?;
        self.log_activity(
            ActivityKind::Session,
            Some(id),
            &format!("Session de {name}"),
            &halcyon_core::util::format_playtime(duration_secs),
        )
    }

    /// Note un lancement (date + compteur). La durée des jeux de plateforme est
    /// relue depuis la plateforme au scan suivant.
    pub fn mark_launched(&self, id: GameId, at: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE games SET last_played = ?2, play_count = play_count + 1 WHERE id = ?1",
            params![id, at],
        )?;
        Ok(())
    }

    /// Associe un visuel local. `by_user = true` le protège des synchronisations.
    pub fn set_artwork(&self, id: GameId, kind: ArtworkKind, path: Option<&Path>, by_user: bool) -> Result<()> {
        let flag = if by_user { kind.override_flag() } else { 0 };
        self.conn.execute(
            &format!("UPDATE games SET {} = ?2, overrides = overrides | ?3 WHERE id = ?1", kind.column()),
            params![id, path.map(|p| p.to_string_lossy().into_owned()), flag],
        )?;
        Ok(())
    }

    /// Supprime un jeu ajouté à la main. Les jeux détectés se masquent plutôt
    /// (sinon ils réapparaîtraient au scan suivant).
    pub fn remove_manual_game(&self, id: GameId) -> Result<()> {
        let n = self.conn.execute("DELETE FROM games WHERE id = ?1 AND source = 'manual'", [id])?;
        if n == 0 {
            return Err(StorageError::Conflict("seuls les jeux ajoutés manuellement peuvent être supprimés".into()));
        }
        Ok(())
    }

    /// Modifie les métadonnées ; elles deviennent prioritaires sur les synchronisations.
    pub fn update_metadata(&self, id: GameId, u: &MetadataUpdate) -> Result<Game> {
        let opt = |s: &str| Some(s.trim().to_string()).filter(|v| !v.is_empty());
        self.conn.execute(
            "UPDATE games SET description = ?2, developer = ?3, publisher = ?4, release_date = ?5, genres = ?6,
                overrides = overrides | ?7, tags = ?8 WHERE id = ?1",
            params![
                id,
                opt(&u.description),
                opt(&u.developer),
                opt(&u.publisher),
                opt(&u.release_date),
                serde_json::to_string(&u.genres)?,
                overrides::DESCRIPTION | overrides::META,
                serde_json::to_string(&u.tags)?
            ],
        )?;
        self.get_game(id)
    }

    /// Complète les métadonnées avec une source en ligne, sans écraser les saisies de l'utilisateur.
    pub fn apply_online_metadata(&self, id: GameId, m: &OnlineMetadata) -> Result<()> {
        let genres = if m.genres.is_empty() { None } else { Some(serde_json::to_string(&m.genres)?) };
        let shots = if m.screenshots.is_empty() { None } else { Some(serde_json::to_string(&m.screenshots)?) };
        let tags = if m.tags.is_empty() { None } else { Some(serde_json::to_string(&m.tags)?) };
        self.conn.execute(
            "UPDATE games SET
                description  = CASE WHEN overrides & ?2 THEN description ELSE COALESCE(?3, description) END,
                developer    = CASE WHEN overrides & ?4 THEN developer ELSE COALESCE(?5, developer) END,
                publisher    = CASE WHEN overrides & ?4 THEN publisher ELSE COALESCE(?6, publisher) END,
                release_date = CASE WHEN overrides & ?4 THEN release_date ELSE COALESCE(?7, release_date) END,
                genres       = CASE WHEN overrides & ?4 THEN genres ELSE COALESCE(?8, genres) END,
                screenshots  = COALESCE(?9, screenshots),
                tags         = CASE WHEN overrides & ?4 THEN tags ELSE COALESCE(?10, tags) END
             WHERE id = ?1",
            params![
                id,
                overrides::DESCRIPTION,
                m.description,
                overrides::META,
                m.developer,
                m.publisher,
                m.release_date,
                genres,
                shots,
                tags
            ],
        )?;
        Ok(())
    }

    /// Émulateur imposé pour un jeu (`None` = émulateur par défaut de la plateforme).
    pub fn set_game_emulator(&self, id: GameId, emulator: Option<&str>) -> Result<()> {
        self.conn.execute("UPDATE games SET emulator = ?2 WHERE id = ?1", params![id, emulator])?;
        Ok(())
    }

    pub fn set_ra_game_id(&self, id: GameId, ra: Option<u32>) -> Result<()> {
        self.conn.execute("UPDATE games SET ra_game_id = ?2 WHERE id = ?1", params![id, ra])?;
        Ok(())
    }

    /// Définit l'URL de jaquette (ex. trouvée via RetroAchievements) si aucune n'existe.
    pub fn set_cover_url_if_missing(&self, id: GameId, url: &str) -> Result<()> {
        self.conn.execute("UPDATE games SET cover_url = ?2 WHERE id = ?1 AND cover_url IS NULL", params![id, url])?;
        Ok(())
    }

    // ---------------------------------------------------------------- collections

    pub fn collections(&self) -> Result<Vec<Collection>> {
        let mut stmt = self.conn.prepare(
            "SELECT c.id, c.name, COUNT(cg.game_id) FROM collections c
             LEFT JOIN collection_games cg ON cg.collection_id = c.id
             GROUP BY c.id ORDER BY c.name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], |r| Ok(Collection { id: r.get(0)?, name: r.get(1)?, count: r.get::<_, i64>(2)? as usize }))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Crée une collection (ou renvoie celle qui porte déjà ce nom).
    pub fn create_collection(&self, name: &str) -> Result<i64> {
        let name = name.trim();
        if name.is_empty() {
            return Err(StorageError::Conflict("le nom de la collection est vide".into()));
        }
        self.conn.execute("INSERT OR IGNORE INTO collections (name) VALUES (?1)", [name])?;
        Ok(self.conn.query_row("SELECT id FROM collections WHERE name = ?1", [name], |r| r.get(0))?)
    }

    pub fn delete_collection(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM collections WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn set_in_collection(&self, collection: i64, game: GameId, member: bool) -> Result<()> {
        if member {
            self.conn.execute(
                "INSERT OR IGNORE INTO collection_games (collection_id, game_id) VALUES (?1, ?2)",
                params![collection, game],
            )?;
        } else {
            self.conn
                .execute("DELETE FROM collection_games WHERE collection_id = ?1 AND game_id = ?2", params![collection, game])?;
        }
        Ok(())
    }

    pub fn collection_members(&self, collection: i64) -> Result<HashSet<GameId>> {
        let mut stmt = self.conn.prepare("SELECT game_id FROM collection_games WHERE collection_id = ?1")?;
        let rows = stmt.query_map([collection], |r| r.get(0))?;
        Ok(rows.collect::<rusqlite::Result<HashSet<_>>>()?)
    }

    pub fn game_collections(&self, game: GameId) -> Result<HashSet<i64>> {
        let mut stmt = self.conn.prepare("SELECT collection_id FROM collection_games WHERE game_id = ?1")?;
        let rows = stmt.query_map([game], |r| r.get(0))?;
        Ok(rows.collect::<rusqlite::Result<HashSet<_>>>()?)
    }

    /// Nombre de sessions et durée totale enregistrées par Halcyon pour un jeu.
    pub fn session_summary(&self, id: GameId) -> Result<(u32, u64)> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(duration_secs), 0) FROM play_sessions WHERE game_id = ?1",
            [id],
            |r| Ok((r.get::<_, i64>(0)? as u32, r.get::<_, i64>(1)? as u64)),
        )?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use halcyon_core::DiscoveredGame;

    fn report(games: Vec<DiscoveredGame>, complete: bool) -> ScanReport {
        ScanReport { games, warnings: vec![], complete }
    }

    #[test]
    fn migrations_apply() {
        let db = Database::open_in_memory().unwrap();
        assert_eq!(db.schema_version().unwrap(), crate::migrations::MIGRATIONS.len() as i64);
    }

    #[test]
    fn collections_and_metadata() {
        let mut db = Database::open_in_memory().unwrap();
        let mut d = DiscoveredGame::new("1", "G");
        d.developer = Some("Studio".into());
        db.merge_scan(GameSource::Steam, &report(vec![d.clone()], true)).unwrap();
        let id = db.load_games().unwrap()[0].id;

        let c = db.create_collection("  Coop ").unwrap();
        assert_eq!(db.create_collection("coop").unwrap(), c, "nom insensible à la casse");
        db.set_in_collection(c, id, true).unwrap();
        assert_eq!(db.collections().unwrap()[0].count, 1);
        assert!(db.collection_members(c).unwrap().contains(&id));
        db.set_in_collection(c, id, false).unwrap();
        assert!(db.game_collections(id).unwrap().is_empty());

        db.update_metadata(id, &MetadataUpdate { developer: "Moi".into(), genres: vec!["RPG".into()], ..Default::default() })
            .unwrap();
        db.apply_online_metadata(id, &OnlineMetadata { developer: Some("Autre".into()), screenshots: vec!["u".into()], ..Default::default() })
            .unwrap();
        db.merge_scan(GameSource::Steam, &report(vec![d], true)).unwrap();
        let g = db.get_game(id).unwrap();
        assert_eq!(g.developer.as_deref(), Some("Moi"), "saisie utilisateur prioritaire");
        assert_eq!(g.genres, ["RPG"]);
        assert_eq!(g.screenshots, ["u"]);
    }

    #[test]
    fn merge_inserts_updates_and_marks_uninstalled() {
        let mut db = Database::open_in_memory().unwrap();
        let mut a = DiscoveredGame::new("10", "Alpha");
        a.playtime_secs = Some(600);
        let b = DiscoveredGame::new("20", "Beta");
        let s = db.merge_scan(GameSource::Steam, &report(vec![a.clone(), b.clone(), b.clone()], true)).unwrap();
        assert_eq!(s, MergeStats { added: 2, updated: 0, uninstalled: 0, new_updates: vec![] });

        // L'utilisateur personnalise Alpha, joue, et met Beta en favori.
        let alpha = db.load_games().unwrap().into_iter().find(|g| g.source_key == "10").unwrap();
        db.update_launch_settings(
            alpha.id,
            &LaunchSettingsUpdate { name: "Alpha (renommé)".into(), ..Default::default() },
        )
        .unwrap();
        db.record_session(alpha.id, 5_000, 1_200).unwrap();

        // Nouveau scan : Beta a disparu, Alpha a un temps de jeu plateforme plus faible.
        a.name = "Alpha v2".into();
        a.playtime_secs = Some(700);
        let s = db.merge_scan(GameSource::Steam, &report(vec![a], true)).unwrap();
        assert_eq!(s, MergeStats { added: 0, updated: 1, uninstalled: 1, new_updates: vec![] });

        let games = db.load_games().unwrap();
        let alpha = games.iter().find(|g| g.source_key == "10").unwrap();
        assert_eq!(alpha.name, "Alpha (renommé)", "le nom personnalisé est conservé");
        assert_eq!(alpha.playtime_secs, 1_800, "max(temps Halcyon, temps plateforme)");
        assert_eq!(alpha.last_played, Some(5_000));
        let beta = games.iter().find(|g| g.source_key == "20").unwrap();
        assert!(!beta.installed, "jeu absent → désinstallé, pas supprimé");
    }

    #[test]
    fn incomplete_scan_does_not_uninstall() {
        let mut db = Database::open_in_memory().unwrap();
        db.merge_scan(GameSource::Epic, &report(vec![DiscoveredGame::new("x", "X")], true)).unwrap();
        let s = db.merge_scan(GameSource::Epic, &report(vec![], false)).unwrap();
        assert_eq!(s.uninstalled, 0);
        assert!(db.load_games().unwrap()[0].installed);
    }

    #[test]
    fn manual_games_are_deduplicated() {
        let mut db = Database::open_in_memory().unwrap();
        let input = ManualGameInput { name: "".into(), executable: "C:/Games/Foo/foo.exe".into(), ..Default::default() };
        let g = db.add_manual(&input).unwrap();
        assert_eq!(g.name, "foo");
        assert!(db.add_manual(&ManualGameInput { executable: "c:\\games\\foo\\FOO.exe".into(), ..input }).is_err());

        // Le scanner standalone ne doit pas recréer le même jeu.
        let d = DiscoveredGame::new(path_key(Path::new("C:/Games/Foo/foo.exe")), "Foo");
        let s = db.merge_scan(GameSource::Standalone, &report(vec![d], true)).unwrap();
        assert_eq!(s.added, 0);

        db.remove_manual_game(g.id).unwrap();
        assert!(db.load_games().unwrap().is_empty());
    }

    #[test]
    fn user_artwork_survives_rescan() {
        let mut db = Database::open_in_memory().unwrap();
        let mut d = DiscoveredGame::new("1", "G");
        d.artwork.cover = Some("steam_cover.jpg".into());
        db.merge_scan(GameSource::Steam, &report(vec![d.clone()], true)).unwrap();
        let id = db.load_games().unwrap()[0].id;
        db.set_artwork(id, ArtworkKind::Cover, Some(Path::new("mine.png")), true).unwrap();
        db.merge_scan(GameSource::Steam, &report(vec![d], true)).unwrap();
        assert_eq!(db.get_game(id).unwrap().artwork.cover, Some(PathBuf::from("mine.png")));
    }
}

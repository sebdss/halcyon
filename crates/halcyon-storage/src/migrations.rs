//! Migrations du schéma, appliquées dans l'ordre selon `PRAGMA user_version`.
//! Ne jamais modifier une migration publiée : en ajouter une nouvelle.

pub const MIGRATIONS: &[&str] = &[
    // v1 — bibliothèque et sessions de jeu
    r#"
    CREATE TABLE games (
        id            INTEGER PRIMARY KEY,
        source        TEXT    NOT NULL,
        source_key    TEXT    NOT NULL,
        name          TEXT    NOT NULL,
        description   TEXT,
        developer     TEXT,
        publisher     TEXT,
        release_date  TEXT,
        genres        TEXT    NOT NULL DEFAULT '[]',
        install_dir   TEXT,
        executable    TEXT,
        arguments     TEXT    NOT NULL DEFAULT '',
        working_dir   TEXT,
        launch_uri    TEXT,
        installed     INTEGER NOT NULL DEFAULT 1,
        favorite      INTEGER NOT NULL DEFAULT 0,
        hidden        INTEGER NOT NULL DEFAULT 0,
        cover_path    TEXT,
        banner_path   TEXT,
        logo_path     TEXT,
        cover_url     TEXT,
        banner_url    TEXT,
        logo_url      TEXT,
        added_at      INTEGER NOT NULL,
        last_played   INTEGER,
        playtime_secs INTEGER NOT NULL DEFAULT 0,
        play_count    INTEGER NOT NULL DEFAULT 0,
        size_bytes    INTEGER,
        overrides     INTEGER NOT NULL DEFAULT 0,
        UNIQUE (source, source_key)
    );

    CREATE TABLE play_sessions (
        id            INTEGER PRIMARY KEY,
        game_id       INTEGER NOT NULL REFERENCES games(id) ON DELETE CASCADE,
        started_at    INTEGER NOT NULL,
        duration_secs INTEGER NOT NULL
    );
    CREATE INDEX idx_sessions_game ON play_sessions(game_id, started_at);
    "#,
    // v2 — émulation, succès, collections, captures d'écran
    r#"
    ALTER TABLE games ADD COLUMN platform TEXT;
    ALTER TABLE games ADD COLUMN emulator TEXT;
    ALTER TABLE games ADD COLUMN ra_game_id INTEGER;
    ALTER TABLE games ADD COLUMN screenshots TEXT NOT NULL DEFAULT '[]';

    CREATE TABLE achievements (
        id                   INTEGER PRIMARY KEY,
        game_id              INTEGER NOT NULL REFERENCES games(id) ON DELETE CASCADE,
        provider             TEXT    NOT NULL,
        api_name             TEXT    NOT NULL,
        name                 TEXT    NOT NULL,
        description          TEXT    NOT NULL DEFAULT '',
        icon_url             TEXT,
        icon_path            TEXT,
        hidden               INTEGER NOT NULL DEFAULT 0,
        points               INTEGER,
        rarity               REAL,
        unlocked             INTEGER NOT NULL DEFAULT 0,
        unlocked_at          INTEGER,
        unlocked_hardcore_at INTEGER,
        progress_cur         REAL,
        progress_max         REAL,
        display_order        INTEGER NOT NULL DEFAULT 0,
        UNIQUE (game_id, provider, api_name)
    );
    CREATE INDEX idx_achievements_unlocked ON achievements(unlocked, unlocked_at);

    CREATE TABLE achievement_sync (
        game_id   INTEGER NOT NULL REFERENCES games(id) ON DELETE CASCADE,
        provider  TEXT    NOT NULL,
        synced_at INTEGER NOT NULL,
        status    TEXT    NOT NULL,
        message   TEXT,
        PRIMARY KEY (game_id, provider)
    );

    CREATE TABLE collections (
        id   INTEGER PRIMARY KEY,
        name TEXT NOT NULL UNIQUE COLLATE NOCASE
    );
    CREATE TABLE collection_games (
        collection_id INTEGER NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
        game_id       INTEGER NOT NULL REFERENCES games(id) ON DELETE CASCADE,
        PRIMARY KEY (collection_id, game_id)
    );
    "#,
    // v3 — suivi personnel, versions, journal d'activité, collections intelligentes,
    //      profils de lancement
    r#"
    ALTER TABLE games ADD COLUMN status TEXT NOT NULL DEFAULT '';
    ALTER TABLE games ADD COLUMN completed_at INTEGER;
    ALTER TABLE games ADD COLUMN notes TEXT NOT NULL DEFAULT '';
    ALTER TABLE games ADD COLUMN playtime_platform_secs INTEGER;
    ALTER TABLE games ADD COLUMN playtime_manual_secs INTEGER;
    ALTER TABLE games ADD COLUMN tags TEXT NOT NULL DEFAULT '[]';
    ALTER TABLE games ADD COLUMN version TEXT;
    ALTER TABLE games ADD COLUMN update_state TEXT NOT NULL DEFAULT '';
    ALTER TABLE games ADD COLUMN update_downloaded INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE games ADD COLUMN update_total INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE games ADD COLUMN installed_at INTEGER;
    UPDATE games SET installed_at = added_at WHERE installed = 1;

    CREATE TABLE version_history (
        id      INTEGER PRIMARY KEY,
        game_id INTEGER NOT NULL REFERENCES games(id) ON DELETE CASCADE,
        version TEXT    NOT NULL,
        seen_at INTEGER NOT NULL
    );
    CREATE INDEX idx_versions_game ON version_history(game_id, seen_at);

    CREATE TABLE activity (
        id      INTEGER PRIMARY KEY,
        at      INTEGER NOT NULL,
        kind    TEXT    NOT NULL,
        game_id INTEGER REFERENCES games(id) ON DELETE SET NULL,
        title   TEXT    NOT NULL,
        detail  TEXT    NOT NULL DEFAULT ''
    );
    CREATE INDEX idx_activity_at ON activity(at);

    CREATE TABLE smart_collections (
        id       INTEGER PRIMARY KEY,
        name     TEXT    NOT NULL UNIQUE COLLATE NOCASE,
        rules    TEXT    NOT NULL,
        position INTEGER NOT NULL DEFAULT 0
    );

    CREATE TABLE launch_profiles (
        id         INTEGER PRIMARY KEY,
        game_id    INTEGER NOT NULL REFERENCES games(id) ON DELETE CASCADE,
        name       TEXT    NOT NULL,
        is_default INTEGER NOT NULL DEFAULT 0,
        data       TEXT    NOT NULL
    );
    CREATE INDEX idx_profiles_game ON launch_profiles(game_id);
    "#,
    // v4 — sauvegardes de jeux et mods
    r#"
    CREATE TABLE save_config (
        game_id      INTEGER PRIMARY KEY REFERENCES games(id) ON DELETE CASCADE,
        locations    TEXT    NOT NULL DEFAULT '[]',
        auto_before  INTEGER NOT NULL DEFAULT 1,
        auto_after   INTEGER NOT NULL DEFAULT 1,
        last_id      TEXT,
        last_hash    TEXT,
        last_at      INTEGER
    );

    CREATE TABLE mod_state (
        game_id        INTEGER PRIMARY KEY REFERENCES games(id) ON DELETE CASCADE,
        adapter        TEXT,
        active_profile INTEGER,
        nexus_domain   TEXT
    );

    CREATE TABLE mod_profiles (
        id       INTEGER PRIMARY KEY,
        game_id  INTEGER NOT NULL REFERENCES games(id) ON DELETE CASCADE,
        name     TEXT    NOT NULL,
        entries  TEXT    NOT NULL DEFAULT '[]',
        updated_at INTEGER NOT NULL DEFAULT 0
    );
    CREATE INDEX idx_mod_profiles_game ON mod_profiles(game_id);
    "#,
    // v5 (Halcyon 0.4) — index seulement : lectures bornées dans le temps (accueil,
    // statistiques de la semaine). Une base v5 reste lisible par la 0.3 (aucune table ni
    // colonne modifiée).
    r#"
    CREATE INDEX IF NOT EXISTS idx_sessions_started ON play_sessions(started_at);
    "#,
];

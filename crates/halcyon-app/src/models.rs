//! Modèles Slint implémentés en Rust.
//!
//! La grille et la liste ne copient pas la bibliothèque : elles lisent
//! `LibraryData` à la demande. Slint n'appelle `row_data` que pour les lignes
//! visibles — c'est ce qui déclenche, paresseusement, le chargement des jaquettes.

use crate::images::{ImageCache, ImageKey, Kind};
use crate::{GameCard, GameDetail, GridRow};
use halcyon_core::util;
use halcyon_core::{Game, GameId, LibraryQuery, PlayStatus, PlaytimeOrigin, QueryContext, UpdateState};
use slint::{Color, Model, ModelNotify, ModelRc, ModelTracker, SharedString, VecModel};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::RwLock;

/// Noms des plateformes émulées (identifiant → (abréviation, nom complet)).
static PLATFORMS: RwLock<Option<HashMap<String, (String, String)>>> = RwLock::new(None);

pub fn set_platform_names(map: HashMap<String, (String, String)>) {
    if let Ok(mut p) = PLATFORMS.write() {
        *p = Some(map);
    }
}

/// Identifiant → nom complet, pour la recherche (`plateforme:snes`).
pub fn platform_full_names() -> HashMap<String, String> {
    PLATFORMS
        .read()
        .ok()
        .and_then(|p| p.as_ref().map(|m| m.iter().map(|(k, v)| (k.clone(), v.1.clone())).collect()))
        .unwrap_or_default()
}

pub fn platform_names(id: &str) -> (String, String) {
    PLATFORMS
        .read()
        .ok()
        .and_then(|p| p.as_ref().and_then(|m| m.get(id).cloned()))
        .unwrap_or_else(|| (id.to_uppercase(), id.to_string()))
}

/// Bibliothèque en mémoire et résultat de la requête courante.
#[derive(Default)]
pub struct LibraryData {
    pub games: Vec<Game>,
    pub by_id: HashMap<GameId, usize>,
    pub query: LibraryQuery,
    /// Indices dans `games`, dans l'ordre d'affichage.
    pub filtered: Vec<usize>,
    /// Position d'un jeu dans `filtered`.
    pub pos_of: HashMap<GameId, usize>,
    pub columns: usize,
    /// Données des filtres avancés (progression des succès, noms de plateformes).
    pub ctx: QueryContext,
}

impl LibraryData {
    pub fn set_games(&mut self, games: Vec<Game>) {
        self.games = games;
        self.reindex();
    }

    pub fn reindex(&mut self) {
        self.by_id = self.games.iter().enumerate().map(|(i, g)| (g.id, i)).collect();
        self.apply_query();
    }

    pub fn apply_query(&mut self) {
        self.ctx.now = halcyon_storage::now_unix();
        self.filtered = self.query.apply_with(&self.games, &self.ctx);
        self.pos_of = self.filtered.iter().enumerate().map(|(pos, &i)| (self.games[i].id, pos)).collect();
    }

    pub fn get(&self, id: GameId) -> Option<&Game> {
        self.by_id.get(&id).map(|&i| &self.games[i])
    }

    pub fn get_mut(&mut self, id: GameId) -> Option<&mut Game> {
        self.by_id.get(&id).copied().map(move |i| &mut self.games[i])
    }

    /// Remplace (ou ajoute) un jeu.
    pub fn upsert(&mut self, game: Game) {
        match self.by_id.get(&game.id) {
            Some(&i) => self.games[i] = game,
            None => {
                self.by_id.insert(game.id, self.games.len());
                self.games.push(game);
            }
        }
    }

    pub fn remove(&mut self, id: GameId) {
        self.games.retain(|g| g.id != id);
        self.reindex();
    }

    pub fn columns(&self) -> usize {
        self.columns.max(1)
    }
}

/// Couleurs de la jaquette générée, dérivées du nom.
pub fn placeholder_colors(name: &str) -> (Color, Color) {
    let (a, b) = util::placeholder_hues(name);
    (hsv(a, 0.55, 0.50), hsv(b, 0.65, 0.22))
}

fn hsv(h: f32, s: f32, v: f32) -> Color {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h as u32) / 60 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let to = |f: f32| ((f + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    Color::from_rgb_u8(to(r), to(g), to(b))
}

/// Provenance du jeu. Le temps de jeu a sa propre place dans les légendes (tuile, liste) :
/// la ligne de source ne le répète pas.
fn source_line(g: &Game) -> String {
    match &g.platform {
        Some(p) => platform_names(p).1,
        None => g.source.label().to_string(),
    }
}

/// Carte d'un jeu (grille, listes, étagères de l'accueil).
pub fn card_for(g: &Game, images: &ImageCache, now: i64) -> GameCard {
    let cover = images.get(ImageKey { game: g.id, kind: Kind::Cover }, g.artwork.cover.as_deref());
    let (a, b) = placeholder_colors(&g.name);
    GameCard {
        id: g.id as i32,
        title: g.name.as_str().into(),
        source: source_line(g).into(),
        has_cover: cover.is_some(),
        cover: cover.unwrap_or_default(),
        color_a: a,
        color_b: b,
        initials: util::initials(&g.name).into(),
        installed: g.installed,
        favorite: g.favorite,
        playtime: util::format_playtime(g.effective_playtime()).into(),
        last_played: g.last_played.map(|t| util::format_relative(t, now)).unwrap_or_else(|| "—".into()).into(),
        badge: g.platform.as_deref().map(|p| platform_names(p).0).unwrap_or_default().into(),
    }
}

fn s(v: &Option<String>) -> SharedString {
    v.as_deref().unwrap_or("").into()
}

fn p(v: &Option<std::path::PathBuf>) -> SharedString {
    v.as_ref().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default().into()
}

/// Données de suivi de la fiche, lues en base.
#[derive(Clone, Copy, Debug, Default)]
pub struct Tracking {
    /// (nombre de sessions, durée totale) suivies par Halcyon.
    pub sessions: (u32, u64),
    pub first_session: Option<i64>,
}

/// Fiche détaillée d'un jeu.
pub fn detail_for(g: &Game, images: &ImageCache, tracking: Tracking, now: i64) -> GameDetail {
    let sessions = tracking.sessions;
    let img = |kind, path: &Option<std::path::PathBuf>| images.get(ImageKey { game: g.id, kind }, path.as_deref());
    let cover = img(Kind::Cover, &g.artwork.cover);
    let banner = img(Kind::Banner, &g.artwork.banner);
    let logo = img(Kind::Logo, &g.artwork.logo);
    let (a, b) = placeholder_colors(&g.name);
    let launch_mode = match g.launch_target() {
        Some(halcyon_core::LaunchTarget::Uri(u)) if u.starts_with("heroic://") => tr!("Via Heroic Games Launcher").to_string(),
        Some(halcyon_core::LaunchTarget::Uri(_)) => tr!("Via le client {label}", label = g.source.label()),
        Some(halcyon_core::LaunchTarget::Process { .. }) => tr!("Via Legendary").to_string(),
        Some(halcyon_core::LaunchTarget::Executable { .. }) => tr!("Exécutable direct"),
        None if g.source == halcyon_core::GameSource::Emulated => tr!("Émulateur"),
        None if g.source == halcyon_core::GameSource::Minecraft => tr!("Onglet Minecraft (installation sélectionnée)"),
        None => tr!("Non configuré"),
    };
    let sessions_text = match sessions {
        (0, _) => tr!("Aucune"),
        (n, secs) => format!("{n} · {}", util::format_playtime(secs)),
    };
    GameDetail {
        id: g.id as i32,
        title: g.name.as_str().into(),
        source: g.source.label().into(),
        description: s(&g.description),
        developer: s(&g.developer),
        publisher: s(&g.publisher),
        release: s(&g.release_date),
        playtime: util::format_playtime(g.effective_playtime()).into(),
        last_played: g.last_played.map(|t| util::format_relative(t, now)).unwrap_or_else(|| tr!("Jamais").into()).into(),
        sessions: sessions_text.into(),
        installed: g.installed,
        install_state: if g.installed { tr!("Installé") } else { tr!("Non installé") }.into(),
        size: g.size_bytes.map(util::format_size).unwrap_or_default().into(),
        install_dir: p(&g.install_dir),
        launch_mode: launch_mode.into(),
        platform_client: g.source.uses_platform_client(),
        executable: if g.source.uses_platform_client() && !g.has_override(halcyon_core::model::overrides::LAUNCH) {
            SharedString::default()
        } else {
            p(&g.executable)
        },
        arguments: g.arguments.as_str().into(),
        working_dir: p(&g.working_dir),
        can_remove: g.source == halcyon_core::GameSource::Manual,
        favorite: g.favorite,
        hidden: g.hidden,
        has_cover: cover.is_some(),
        cover: cover.unwrap_or_default(),
        has_banner: banner.is_some(),
        banner: banner.unwrap_or_default(),
        has_logo: logo.is_some(),
        logo: logo.unwrap_or_default(),
        color_a: a,
        color_b: b,
        initials: util::initials(&g.name).into(),
        genres: g.genres.join(", ").into(),
        platform: g.platform.as_deref().map(|p| platform_names(p).1).unwrap_or_default().into(),
        is_emulated: g.source == halcyon_core::GameSource::Emulated,
        can_install: g.source == halcyon_core::GameSource::Steam && !g.installed,
        ra_id: g.ra_game_id.map(|i| i.to_string()).unwrap_or_default().into(),
        status: PlayStatus::ALL.iter().position(|s| *s == g.status).unwrap_or(0) as i32,
        notes: g.notes.as_str().into(),
        tags: g.tags.iter().map(|t| halcyon_i18n::tc("tag", t)).collect::<Vec<_>>().join(", ").into(),
        playtime_origin: match g.playtime_origin() {
            PlaytimeOrigin::Platform => tr!("selon {label}", label = g.source.label()),
            o => o.label().to_string(),
        }
        .into(),
        playtime_platform: match g.playtime_platform_secs {
            Some(s) if g.source.uses_platform_client() => util::format_playtime(s),
            _ if g.source.uses_platform_client() => tr!("Non fourni").into(),
            _ => tr!("Sans objet").into(),
        }
        .into(),
        playtime_local: match sessions {
            (0, _) => tr!("Aucune session"),
            (n, secs) => trn!(n, "{time} · {n} session", "{time} · {n} sessions", time = util::format_playtime(secs)),
        }
        .into(),
        playtime_manual: g.playtime_manual_secs.map(util::format_playtime).unwrap_or_default().into(),
        has_manual: g.playtime_manual_secs.is_some(),
        first_session: tracking.first_session.map(util::format_date).unwrap_or_else(|| "—".into()).into(),
        avg_session: match sessions {
            (0, _) => "—".to_string(),
            (n, secs) => util::format_playtime(secs / n as u64),
        }
        .into(),
        version: g.version.clone().unwrap_or_default().into(),
        update_pending: g.update.state.pending(),
        update_state: match g.update.state {
            UpdateState::None if !g.source.uses_platform_client() => tr!("Non suivi pour cette source"),
            UpdateState::Downloading if g.update.total > 0 => tr!("Téléchargement : {downloaded} / {total}", downloaded = util::format_size(g.update.downloaded), total = util::format_size(g.update.total)),
            s => s.label().to_string(),
        }
        .into(),
        update_progress: if g.update.total > 0 { g.update.downloaded as f32 / g.update.total as f32 } else { 0.0 },
    }
}

/// Grille virtualisée : une ligne Slint = `columns` jaquettes.
pub struct GridModel {
    lib: Rc<RefCell<LibraryData>>,
    images: Rc<ImageCache>,
    notify: ModelNotify,
}

impl GridModel {
    pub fn new(lib: Rc<RefCell<LibraryData>>, images: Rc<ImageCache>) -> Self {
        Self { lib, images, notify: ModelNotify::default() }
    }

    pub fn reset(&self) {
        self.notify.reset();
    }

    /// Signale qu'un jeu a changé (image chargée, favori…).
    pub fn game_changed(&self, id: GameId) {
        let row = {
            let lib = self.lib.borrow();
            lib.pos_of.get(&id).map(|pos| pos / lib.columns())
        };
        if let Some(row) = row {
            self.notify.row_changed(row);
        }
    }
}

impl Model for GridModel {
    type Data = GridRow;

    fn row_count(&self) -> usize {
        let lib = self.lib.borrow();
        lib.filtered.len().div_ceil(lib.columns())
    }

    fn row_data(&self, row: usize) -> Option<GridRow> {
        let lib = self.lib.borrow();
        let cols = lib.columns();
        let start = row * cols;
        if start >= lib.filtered.len() {
            return None;
        }
        let end = (start + cols).min(lib.filtered.len());
        let now = halcyon_storage::now_unix();
        let cards: Vec<GameCard> =
            lib.filtered[start..end].iter().map(|&i| card_for(&lib.games[i], &self.images, now)).collect();
        Some(GridRow { cards: ModelRc::new(VecModel::from(cards)) })
    }

    fn model_tracker(&self) -> &dyn ModelTracker {
        &self.notify
    }
}

/// Liste virtualisée : une ligne = un jeu.
pub struct ListModel {
    lib: Rc<RefCell<LibraryData>>,
    images: Rc<ImageCache>,
    notify: ModelNotify,
}

impl ListModel {
    pub fn new(lib: Rc<RefCell<LibraryData>>, images: Rc<ImageCache>) -> Self {
        Self { lib, images, notify: ModelNotify::default() }
    }

    pub fn reset(&self) {
        self.notify.reset();
    }

    pub fn game_changed(&self, id: GameId) {
        let pos = self.lib.borrow().pos_of.get(&id).copied();
        if let Some(pos) = pos {
            self.notify.row_changed(pos);
        }
    }
}

impl Model for ListModel {
    type Data = GameCard;

    fn row_count(&self) -> usize {
        self.lib.borrow().filtered.len()
    }

    fn row_data(&self, row: usize) -> Option<GameCard> {
        let lib = self.lib.borrow();
        let &i = lib.filtered.get(row)?;
        Some(card_for(&lib.games[i], &self.images, halcyon_storage::now_unix()))
    }

    fn model_tracker(&self) -> &dyn ModelTracker {
        &self.notify
    }
}

//! Suivi personnel, statistiques, journal d'activité, mises à jour et collections
//! intelligentes.

use crate::controller::Controller;
use crate::ctl_bigpicture::local_offset_secs;
use crate::models::card_for;
use crate::{
    ActivityRow, AppUi, BarItem, DetailUi, JournalUi, LibraryUi, Nav, Page, RuleRow, SessionRow, SmartRow, SmartUi, StatsUi,
    TopGame, UpdateRow, UpdatesUi, VersionRow,
};
use halcyon_core::rules::{ParamKind, Rule, RuleSet, KINDS};
use halcyon_core::search::parse_duration;
use halcyon_core::stats::{self, Bucket};
use halcyon_core::{util, Game, GameId, GameSource, PlayStatus, PlaytimeOrigin, UpdateState};
use halcyon_integrations::{launcher, steam};
use halcyon_storage::activity::CATEGORIES;
use halcyon_storage::{now_unix, ActivityKind};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::rc::Rc;

/// Collection intelligente en cours d'édition.
#[derive(Default)]
pub(crate) struct SmartEdit {
    pub id: i64,
    pub rules: Vec<(usize, String, bool)>,
    pub match_all: bool,
}

/// Dossiers de prérequis livrés avec les jeux (DirectX, Visual C++, .NET…).
const PREREQ_DIRS: &[&str] = &["_CommonRedist", "_Redist", "Redist", "redist", "Prerequisites", "_Prerequisites"];

fn short_duration(secs: u64) -> String {
    if secs >= 3600 {
        format!("{} h", (secs as f64 / 3600.0).round() as u64)
    } else if secs >= 60 {
        tr!("{v} min", v = secs / 60)
    } else if secs > 0 {
        tr!("< 1 min")
    } else {
        String::new()
    }
}

fn bars(buckets: &[Bucket], fmt: impl Fn(u64) -> String) -> Vec<BarItem> {
    let ratios = stats::ratios(buckets);
    buckets
        .iter()
        .zip(ratios)
        .map(|(b, r)| BarItem { label: b.label.as_str().into(), value: fmt(b.value).into(), ratio: r })
        .collect()
}

/// « 3 nov. 2026 · 21:04 » en heure locale.
pub(crate) fn date_time(t: i64) -> String {
    let local = t + local_offset_secs();
    let s = local.rem_euclid(86_400);
    format!("{} · {:02}:{:02}", util::format_date(local), s / 3600, s / 60 % 60)
}

impl Controller {
    // ------------------------------------------------------------------ confirmation

    /// Demande confirmation avant une opération destructive.
    pub(crate) fn confirm(&self, title: &str, text: &str, button: &str, action: impl FnOnce(&Rc<Controller>) + 'static) {
        let Some(app) = self.app() else { return };
        *self.pending_confirm.borrow_mut() = Some(Box::new(action));
        let ui = app.global::<AppUi>();
        ui.set_confirm_title(title.into());
        ui.set_confirm_text(text.into());
        ui.set_confirm_button(button.into());
        ui.set_confirm_open(true);
    }

    pub(crate) fn on_confirm_accepted(self: &Rc<Self>) {
        let action = self.pending_confirm.borrow_mut().take();
        if let Some(action) = action {
            self.sound(halcyon_audio::Cue::Confirm);
            action(self);
        }
    }

    // ------------------------------------------------------------------ fiche : suivi

    pub(crate) fn refresh_tracking_ui(&self, g: &Game) {
        let Some(app) = self.app() else { return };
        let dui = app.global::<DetailUi>();
        let sessions = self.db().sessions().unwrap_or_default();
        let rows: Vec<SessionRow> = sessions
            .iter()
            .filter(|s| s.game_id == g.id)
            .take(25)
            .map(|s| SessionRow {
                game_id: g.id as i32,
                game: g.name.as_str().into(),
                date: date_time(s.started_at).into(),
                duration: util::format_playtime(s.duration_secs).into(),
            })
            .collect();
        dui.set_sessions(ModelRc::new(VecModel::from(rows)));
        let versions: Vec<VersionRow> = self
            .db()
            .version_history(g.id)
            .unwrap_or_default()
            .into_iter()
            .take(15)
            .map(|(v, at)| VersionRow {
                game_id: g.id as i32,
                game: g.name.as_str().into(),
                version: v.into(),
                date: util::format_date(at).into(),
            })
            .collect();
        dui.set_versions(ModelRc::new(VecModel::from(versions)));

        let mut prereqs: Vec<SharedString> = Vec::new();
        let mut prereq_dir = String::new();
        if let Some(dir) = g.install_dir.as_deref() {
            for name in PREREQ_DIRS {
                let d = dir.join(name);
                if let Ok(entries) = std::fs::read_dir(&d) {
                    prereq_dir = d.to_string_lossy().into_owned();
                    let mut names: Vec<String> = entries.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
                    names.sort();
                    prereqs.extend(names.into_iter().take(12).map(SharedString::from));
                    break;
                }
            }
        }
        dui.set_prereqs(ModelRc::new(VecModel::from(prereqs)));
        dui.set_prereq_dir(prereq_dir.into());
        dui.set_prereq_note(
            match g.source {
                GameSource::Steam => tr!("Installés automatiquement par Steam au premier lancement si nécessaire."),
                GameSource::Epic => tr!("Normalement installés par le client Epic / Heroic."),
                _ => tr!("Si le jeu ne démarre pas, installez ces composants depuis ce dossier."),
            }
            .into(),
        );
    }

    pub(crate) fn set_status(&self, id: GameId, index: usize) {
        let status = PlayStatus::ALL.get(index).copied().unwrap_or_default();
        if let Err(e) = self.db().set_status(id, status) {
            return self.toast(tr!("Enregistrement impossible : {e}", e = e), true);
        }
        let game = self.db().get_game(id);
        if let Ok(g) = game {
            self.replace_game(g);
        }
        self.after_tracking_change();
    }

    pub(crate) fn save_notes(&self, id: GameId, notes: &str) {
        if let Err(e) = self.db().set_notes(id, notes) {
            return self.toast(tr!("Enregistrement impossible : {e}", e = e), true);
        }
        if let Some(g) = self.lib.borrow_mut().get_mut(id) {
            g.notes = notes.to_string();
        }
        self.refresh_detail();
        self.toast(tr!("Notes enregistrées"), false);
    }

    pub(crate) fn set_manual_playtime(&self, id: GameId, text: &str) {
        let value = if text.trim().is_empty() {
            None
        } else {
            match parse_duration(text) {
                Some(v) => Some(v),
                None => return self.toast(tr!("Durée non reconnue : utilisez par exemple 12h30, 45m ou 3"), true),
            }
        };
        if let Err(e) = self.db().set_manual_playtime(id, value) {
            return self.toast(tr!("Enregistrement impossible : {e}", e = e), true);
        }
        if let Some(g) = self.lib.borrow_mut().get_mut(id) {
            g.playtime_manual_secs = value;
        }
        self.game_changed(id);
        self.after_tracking_change();
        self.toast(if value.is_some() { tr!("Temps de jeu corrigé") } else { tr!("Temps calculé rétabli") }, false);
    }

    /// Le statut ou le temps d'un jeu a changé : collections et vues dépendantes.
    fn after_tracking_change(&self) {
        self.refresh_filters();
        self.refresh_library();
        self.refresh_home();
    }

    // ------------------------------------------------------------------ statistiques

    pub(crate) fn refresh_stats(&self) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<StatsUi>();
        let now = now_unix();
        let tz = local_offset_secs();
        let sessions = self.db().sessions().unwrap_or_default();
        let unlocks = self.db().unlock_times().unwrap_or_default();
        let lib = self.lib.borrow();
        let games: Vec<&Game> = lib.games.iter().filter(|g| !g.hidden).collect();

        let total: u64 = games.iter().map(|g| g.effective_playtime()).sum();
        let platform: u64 = games.iter().filter_map(|g| g.playtime_platform_secs).sum();
        let local: u64 = sessions.iter().map(|s| s.duration_secs).sum();
        let manual = games.iter().filter(|g| g.playtime_manual_secs.is_some()).count();
        ui.set_total(util::format_playtime(total).into());
        ui.set_total_detail(
            trn!(
                manual,
                "Plateformes : {platform} · suivi local Halcyon : {local} · {n} correction manuelle. Pour chaque jeu, le temps retenu est la correction manuelle si elle existe, sinon le plus élevé entre la plateforme et le suivi local (sans double compte).",
                "Plateformes : {platform} · suivi local Halcyon : {local} · {n} corrections manuelles. Pour chaque jeu, le temps retenu est la correction manuelle si elle existe, sinon le plus élevé entre la plateforme et le suivi local (sans double compte).",
                platform = util::format_playtime(platform),
                local = util::format_playtime(local),
            )
            .into(),
        );
        ui.set_games_played(games.iter().filter(|g| g.effective_playtime() > 0 || g.last_played.is_some()).count() as i32);
        ui.set_sessions(sessions.len() as i32);
        ui.set_avg_session(if sessions.is_empty() { "—".into() } else { util::format_playtime(local / sessions.len() as u64).into() });
        ui.set_local_total(util::format_playtime(local).into());
        ui.set_has_local(!sessions.is_empty());
        ui.set_completed(games.iter().filter(|g| g.status == PlayStatus::Completed).count() as i32);

        let items = || sessions.iter().map(|s| (s.started_at, s.duration_secs));
        let weeks = stats::weekly(items(), now, tz, 12);
        let week = weeks.last().map(|b| b.value).unwrap_or(0);
        ui.set_week_total(if week == 0 { "0 h".into() } else { short_duration(week).into() });
        ui.set_weeks(ModelRc::new(VecModel::from(bars(&weeks, short_duration))));
        ui.set_months(ModelRc::new(VecModel::from(bars(&stats::monthly(items(), now, tz, 12), short_duration))));
        ui.set_streak(stats::streak(sessions.iter().map(|s| s.started_at), now, tz) as i32);
        let count = |n: u64| if n == 0 { String::new() } else { n.to_string() };
        ui.set_unlocks(ModelRc::new(VecModel::from(bars(&stats::monthly(unlocks.iter().map(|&t| (t, 1)), now, tz, 12), count))));
        let completions = games.iter().filter_map(|g| g.completed_at).map(|t| (t, 1));
        ui.set_completions(ModelRc::new(VecModel::from(bars(&stats::monthly(completions, now, tz, 12), count))));

        let mut top: Vec<&&Game> = games.iter().filter(|g| g.effective_playtime() > 0).collect();
        top.sort_by_key(|g| std::cmp::Reverse(g.effective_playtime()));
        let max = top.first().map(|g| g.effective_playtime()).unwrap_or(1).max(1);
        let top_rows: Vec<TopGame> = top
            .iter()
            .take(10)
            .map(|g| TopGame {
                id: g.id as i32,
                title: g.name.as_str().into(),
                value: util::format_playtime(g.effective_playtime()).into(),
                origin: match g.playtime_origin() {
                    PlaytimeOrigin::Platform => g.source.label().to_string(),
                    o => o.label().to_string(),
                }
                .into(),
                ratio: g.effective_playtime() as f32 / max as f32,
                card: card_for(g, &self.images, now),
            })
            .collect();
        ui.set_top(ModelRc::new(VecModel::from(top_rows)));

        let mut by_source: Vec<(String, u64)> = vec![("Steam".into(), 0), ("Epic Games".into(), 0), (tr!("Émulation"), 0), (tr!("Autres"), 0)];
        for g in &games {
            let i = match g.source {
                GameSource::Steam => 0,
                GameSource::Epic => 1,
                GameSource::Emulated => 2,
                _ => 3,
            };
            by_source[i].1 += g.effective_playtime();
        }
        let smax = by_source.iter().map(|s| s.1).max().unwrap_or(0).max(1);
        let sources: Vec<BarItem> = by_source
            .iter()
            .map(|(name, v)| BarItem {
                label: name.as_str().into(),
                value: if *v == 0 { "—".into() } else { util::format_playtime(*v).into() },
                ratio: *v as f32 / smax as f32,
            })
            .collect();
        ui.set_sources(ModelRc::new(VecModel::from(sources)));

        let recent: Vec<SessionRow> = sessions
            .iter()
            .take(30)
            .filter_map(|s| {
                let g = lib.get(s.game_id)?;
                Some(SessionRow {
                    game_id: g.id as i32,
                    game: g.name.as_str().into(),
                    date: date_time(s.started_at).into(),
                    duration: util::format_playtime(s.duration_secs).into(),
                })
            })
            .collect();
        ui.set_recent(ModelRc::new(VecModel::from(recent)));
    }

    // ------------------------------------------------------------------ journal

    pub(crate) fn refresh_journal(&self) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<JournalUi>();
        {
            let mut cats = self.journal_cats.borrow_mut();
            if cats.len() != CATEGORIES.len() {
                *cats = vec![false; CATEGORIES.len()];
            }
            ui.set_categories(ModelRc::new(VecModel::from(
                CATEGORIES.iter().map(|(n, _)| SharedString::from(halcyon_i18n::t(n))).collect::<Vec<_>>(),
            )));
            ui.set_selected(ModelRc::new(VecModel::from(cats.clone())));
        }
        let selected: Vec<usize> =
            self.journal_cats.borrow().iter().enumerate().filter(|(_, v)| **v).map(|(i, _)| i).collect();
        let search = ui.get_search().to_string();
        let entries = self.db().activities(&selected, &search, 500).unwrap_or_default();
        let now = now_unix();
        let tz = local_offset_secs();
        let mut last_day = i64::MIN;
        let rows: Vec<ActivityRow> = entries
            .iter()
            .map(|a| {
                let day = (a.at + tz).div_euclid(86_400);
                let new_day = day != last_day;
                last_day = day;
                let s = (a.at + tz).rem_euclid(86_400);
                ActivityRow {
                    id: a.id as i32,
                    game_id: a.game_id.unwrap_or(0) as i32,
                    category: a.kind.category() as i32,
                    title: a.title.as_str().into(),
                    detail: a.detail.as_str().into(),
                    day: util::format_relative(a.at + tz, now + tz).into(),
                    time: format!("{:02}:{:02}", s / 3600, s / 60 % 60).into(),
                    new_day,
                }
            })
            .collect();
        ui.set_count(rows.len() as i32);
        ui.set_total(self.db().activity_count().unwrap_or(0) as i32);
        ui.set_entries(ModelRc::new(VecModel::from(rows)));
    }

    pub(crate) fn toggle_journal_category(&self, i: usize) {
        if let Some(v) = self.journal_cats.borrow_mut().get_mut(i) {
            *v = !*v;
        }
        self.refresh_journal();
    }

    pub(crate) fn clear_journal_filters(&self) {
        self.journal_cats.borrow_mut().iter_mut().for_each(|v| *v = false);
        if let Some(app) = self.app() {
            app.global::<JournalUi>().set_search("".into());
        }
        self.refresh_journal();
    }

    // ------------------------------------------------------------------ mises à jour

    pub(crate) fn refresh_updates(&self) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<UpdatesUi>();
        let now = now_unix();
        let lib = self.lib.borrow();
        let mut pending: Vec<&Game> = lib.games.iter().filter(|g| g.update.state.pending() && !g.hidden).collect();
        pending.sort_by_key(|g| (g.update.state != UpdateState::Downloading, g.sort_key.clone()));
        let rows: Vec<UpdateRow> = pending
            .iter()
            .map(|g| {
                let u = g.update;
                UpdateRow {
                    id: g.id as i32,
                    title: g.name.as_str().into(),
                    source: g.source.label().into(),
                    state: u.state.label().into(),
                    downloading: u.state == UpdateState::Downloading,
                    progress: if u.total > 0 { u.downloaded as f32 / u.total as f32 } else { 0.0 },
                    progress_text: if u.total > 0 {
                        tr!("{downloaded} sur {total}", downloaded = util::format_size(u.downloaded), total = util::format_size(u.total))
                    } else {
                        String::new()
                    }
                    .into(),
                    version: g.version.as_deref().map(|v| tr!("Version installée : {v}", v = v)).unwrap_or_default().into(),
                    client: client_label(g).into(),
                    card: card_for(g, &self.images, now),
                }
            })
            .collect();
        ui.set_downloading(rows.iter().filter(|r| r.downloading).count() as i32);
        ui.set_pending(ModelRc::new(VecModel::from(rows)));
        drop(lib);
        let changes: Vec<VersionRow> = self
            .db()
            .recent_version_changes(60)
            .unwrap_or_default()
            .into_iter()
            .map(|c| VersionRow {
                game_id: c.game_id as i32,
                game: c.game.into(),
                version: c.version.into(),
                date: util::format_date(c.seen_at).into(),
            })
            .collect();
        ui.set_changes(ModelRc::new(VecModel::from(changes)));
        ui.set_notify(self.settings.borrow().update_notifications);
    }

    pub(crate) fn open_update_client(&self, id: GameId) {
        let Some(g) = self.lib.borrow().get(id).cloned() else { return };
        let uri = client_uri(&g);
        match uri {
            Some(u) => {
                if let Err(e) = launcher::open_uri(u) {
                    self.toast(tr!("Client introuvable : {e}", e = e), true);
                }
            }
            None => self.toast(tr!("Ce client ne propose pas de page de téléchargements"), true),
        }
    }

    pub(crate) fn set_update_notify(&self, v: bool) {
        self.settings.borrow_mut().update_notifications = v;
        self.save_settings();
        self.refresh_updates();
    }

    /// Notifie les mises à jour détectées par la dernière analyse.
    pub(crate) fn notify_new_updates(&self, list: &[(GameId, String)]) {
        if list.is_empty() || !self.settings.borrow().update_notifications {
            return;
        }
        let body = if list.len() == 1 {
            list[0].1.clone()
        } else {
            let names: Vec<&str> = list.iter().take(3).map(|(_, n)| n.as_str()).collect();
            let more = list.len().saturating_sub(3);
            if more > 0 { trn!(more, "{names} et {n} autre", "{names} et {n} autres", names = names.join(", ")) } else { names.join(", ") }
        };
        let title = if list.len() == 1 { tr!("Mise à jour disponible") } else { tr!("{list} mises à jour disponibles", list = list.len()) };
        crate::notify::info(&title, &body);
    }

    /// Suit les téléchargements Steam en cours (manifestes relus toutes les 8 s).
    pub(crate) fn ensure_update_polling(self: &Rc<Self>) {
        let downloading = self
            .lib
            .borrow()
            .games
            .iter()
            .any(|g| g.source == GameSource::Steam && g.update.state == UpdateState::Downloading);
        let mut timer = self.update_timer.borrow_mut();
        if !downloading {
            *timer = None;
            return;
        }
        if timer.is_some() {
            return;
        }
        let t = slint::Timer::default();
        let weak = Rc::downgrade(self);
        t.start(slint::TimerMode::Repeated, std::time::Duration::from_secs(8), move || {
            if let Some(c) = weak.upgrade() {
                c.poll_updates();
            }
        });
        *timer = Some(t);
    }

    fn poll_updates(self: &Rc<Self>) {
        let Some(root) = steam::SteamScanner::new(self.settings.borrow().steam_path.clone()).root() else { return };
        let ids: Vec<(GameId, String)> = self
            .lib
            .borrow()
            .games
            .iter()
            .filter(|g| g.source == GameSource::Steam && g.update.state == UpdateState::Downloading)
            .map(|g| (g.id, g.source_key.clone()))
            .collect();
        let mut finished = false;
        for (id, appid) in ids {
            let Some(d) = steam::read_app(&root, &appid) else { continue };
            let info = d.update.unwrap_or_default();
            let changed = self.lib.borrow().get(id).is_some_and(|g| g.update != info);
            if changed {
                let _ = self.db().set_update_progress(id, info);
                if let Some(g) = self.lib.borrow_mut().get_mut(id) {
                    g.update = info;
                }
                self.game_changed(id);
                finished |= !info.state.pending();
            }
        }
        self.refresh_updates_if_visible();
        if finished {
            self.sound(halcyon_audio::Cue::Update);
            // Nouvelle version installée : l'analyse met à jour l'historique et le journal.
            self.start_scan();
        }
        self.ensure_update_polling();
    }

    /// La page et le compteur de la barre latérale dépendent des mêmes données.
    pub(crate) fn refresh_updates_if_visible(&self) {
        self.refresh_updates();
    }

    // ------------------------------------------------------------------ collections intelligentes

    pub(crate) fn reload_smart(&self) {
        let list = self.db().smart_collections().unwrap_or_default();
        *self.smart.borrow_mut() = list;
    }

    /// Nombre de jeux de chaque collection intelligente.
    pub(crate) fn smart_counts(&self) -> Vec<usize> {
        let lib = self.lib.borrow();
        let mut ctx = lib.ctx.clone();
        ctx.now = now_unix();
        self.smart.borrow().iter().map(|s| lib.games.iter().filter(|g| s.rules.matches(g, &ctx)).count()).collect()
    }

    pub(crate) fn smart_members(&self, index: usize) -> Option<std::collections::HashSet<GameId>> {
        let smart = self.smart.borrow();
        let s = smart.get(index)?;
        let lib = self.lib.borrow();
        let mut ctx = lib.ctx.clone();
        ctx.now = now_unix();
        Some(lib.games.iter().filter(|g| s.rules.matches(g, &ctx)).map(|g| g.id).collect())
    }

    pub(crate) fn refresh_smart_ui(&self) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<SmartUi>();
        let counts = self.smart_counts();
        let rows: Vec<SmartRow> = self
            .smart
            .borrow()
            .iter()
            .zip(counts)
            .map(|(s, n)| SmartRow { id: s.id as i32, name: halcyon_i18n::t(&s.name).into(), description: s.rules.describe().into(), count: n as i32 })
            .collect();
        ui.set_collections(ModelRc::new(VecModel::from(rows)));
        ui.set_kind_labels(ModelRc::new(VecModel::from(KINDS.iter().map(|(l, _)| SharedString::from(halcyon_i18n::t(l))).collect::<Vec<_>>())));
        ui.set_kind_params(ModelRc::new(VecModel::from(
            KINDS
                .iter()
                .map(|(_, p)| match p {
                    ParamKind::None => 0,
                    ParamKind::Number => 1,
                    ParamKind::Status => 2,
                    ParamKind::Source => 3,
                    ParamKind::Text => 4,
                })
                .collect::<Vec<i32>>(),
        )));
        ui.set_status_labels(ModelRc::new(VecModel::from(
            PlayStatus::ALL.iter().map(|s| SharedString::from(s.label())).collect::<Vec<_>>(),
        )));
        ui.set_source_labels(ModelRc::new(VecModel::from(
            GameSource::ALL.map(|s| SharedString::from(s.label())).to_vec(),
        )));
        ui.set_rules(ModelRc::from(self.smart_rules.clone()));
    }

    fn smart_rule_rows(&self) {
        let edit = self.smart_edit.borrow();
        let rows: Vec<RuleRow> = edit
            .rules
            .iter()
            .map(|(k, p, n)| RuleRow { kind: *k as i32, param: p.as_str().into(), negate: *n, valid: Rule::from_parts(*k, p, *n).is_some() })
            .collect();
        drop(edit);
        // Mise à jour ligne à ligne : la saisie en cours n'est pas interrompue.
        if self.smart_rules.row_count() == rows.len() {
            for (i, r) in rows.into_iter().enumerate() {
                if self.smart_rules.row_data(i).as_ref() != Some(&r) {
                    self.smart_rules.set_row_data(i, r);
                }
            }
        } else {
            self.smart_rules.set_vec(rows);
        }
        self.smart_preview();
    }

    fn current_ruleset(&self) -> Result<RuleSet, String> {
        let edit = self.smart_edit.borrow();
        let mut rules = Vec::new();
        for (i, (k, p, n)) in edit.rules.iter().enumerate() {
            match Rule::from_parts(*k, p, *n) {
                Some(r) => rules.push(r),
                None => return Err(tr!("Règle {v} : valeur manquante ou invalide", v = i + 1)),
            }
        }
        if rules.is_empty() {
            return Err(tr!("Ajoutez au moins une règle"));
        }
        Ok(RuleSet { match_all: edit.match_all, rules })
    }

    fn smart_preview(&self) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<SmartUi>();
        match self.current_ruleset() {
            Ok(set) => {
                let lib = self.lib.borrow();
                let mut ctx = lib.ctx.clone();
                ctx.now = now_unix();
                ui.set_preview_count(lib.games.iter().filter(|g| set.matches(g, &ctx)).count() as i32);
                ui.set_error("".into());
            }
            Err(e) => ui.set_error(e.into()),
        }
    }

    pub(crate) fn smart_create(&self) {
        let Some(app) = self.app() else { return };
        *self.smart_edit.borrow_mut() = SmartEdit { id: 0, rules: vec![(0, String::new(), false)], match_all: true };
        let ui = app.global::<SmartUi>();
        ui.set_name("".into());
        ui.set_match_all(true);
        self.smart_rules.set_vec(Vec::new());
        self.smart_rule_rows();
        ui.set_editing_id(0);
    }

    pub(crate) fn smart_edit_existing(&self, id: i64) {
        let Some(app) = self.app() else { return };
        let Some(s) = self.smart.borrow().iter().find(|s| s.id == id).cloned() else { return };
        *self.smart_edit.borrow_mut() =
            SmartEdit { id, rules: s.rules.rules.iter().map(Rule::parts).collect(), match_all: s.rules.match_all };
        let ui = app.global::<SmartUi>();
        ui.set_name(halcyon_i18n::t(&s.name).into());
        ui.set_match_all(s.rules.match_all);
        self.smart_rules.set_vec(Vec::new());
        self.smart_rule_rows();
        ui.set_editing_id(id as i32);
    }

    pub(crate) fn smart_set_rule(&self, index: usize, kind: usize, param: &str, negate: bool) {
        {
            let mut edit = self.smart_edit.borrow_mut();
            let Some(r) = edit.rules.get_mut(index) else { return };
            let param = if r.0 != kind {
                // Valeur par défaut adaptée au nouveau type de règle.
                match KINDS.get(kind).map(|k| k.1) {
                    Some(ParamKind::Number) => "7".to_string(),
                    Some(ParamKind::Status) => "completed".to_string(),
                    Some(ParamKind::Source) => "steam".to_string(),
                    _ => param.to_string(),
                }
            } else {
                param.to_string()
            };
            *r = (kind, param, negate);
        }
        self.smart_rule_rows();
    }

    pub(crate) fn smart_add_rule(&self) {
        self.smart_edit.borrow_mut().rules.push((1, "7".into(), false));
        self.smart_rule_rows();
    }

    pub(crate) fn smart_remove_rule(&self, index: usize) {
        {
            let mut edit = self.smart_edit.borrow_mut();
            if index < edit.rules.len() {
                edit.rules.remove(index);
            }
        }
        self.smart_rule_rows();
    }

    pub(crate) fn smart_set_match_all(&self, v: bool) {
        self.smart_edit.borrow_mut().match_all = v;
        if let Some(app) = self.app() {
            app.global::<SmartUi>().set_match_all(v);
        }
        self.smart_preview();
    }

    pub(crate) fn smart_save(&self) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<SmartUi>();
        let set = match self.current_ruleset() {
            Ok(s) => s,
            Err(e) => return ui.set_error(e.into()),
        };
        let id = self.smart_edit.borrow().id;
        let mut name = ui.get_name().to_string();
        // Collection proposée par défaut dont le nom n'a pas été modifié : garder le nom
        // source pour qu'elle reste traduite dans toutes les langues.
        if let Some(s) = self.smart.borrow().iter().find(|s| s.id == id) {
            if name.trim() == halcyon_i18n::t(&s.name) {
                name = s.name.clone();
            }
        }
        let name_shown = halcyon_i18n::t(&name);
        let result = self.db().save_smart_collection(id, &name, &set);
        match result {
            Ok(_) => {
                self.reload_smart();
                self.refresh_filters();
                self.refresh_library();
                self.refresh_smart_ui();
                ui.set_editing_id(-1);
                let _ = self.db().log_activity(ActivityKind::Settings, None, &tr_later!("Collection intelligente « {name} » enregistrée", name = name_shown.trim()), &set.describe());
                self.toast(tr!("Collection « {name} » enregistrée", name = name_shown.trim()), false);
            }
            Err(e) => ui.set_error(e.to_string().into()),
        }
    }

    pub(crate) fn smart_remove(&self, id: i64) {
        let Some(s) = self.smart.borrow().iter().find(|s| s.id == id).cloned() else { return };
        if let Some(app) = self.app() {
            app.global::<SmartUi>().set_open(false);
        }
        self.confirm(
            &tr!("Supprimer la collection intelligente ?"),
            &tr!("« {name} » sera supprimée. Aucun jeu n'est retiré de la bibliothèque.", name = halcyon_i18n::t(&s.name)),
            &tr!("Supprimer"),
            move |c| {
                if let Err(e) = c.db().delete_smart_collection(id) {
                    return c.toast(tr!("Suppression impossible : {e}", e = e), true);
                }
                c.reload_smart();
                c.refresh_filters();
                c.refresh_library();
                c.refresh_smart_ui();
                if let Some(app) = c.app() {
                    app.global::<SmartUi>().set_open(true);
                }
                c.toast(tr!("Collection « {name} » supprimée", name = halcyon_i18n::t(&s.name)), false);
            },
        );
    }

    /// Affiche une collection intelligente dans la bibliothèque.
    pub(crate) fn smart_show(&self, id: i64) {
        let Some(app) = self.app() else { return };
        let Some(pos) = self.smart.borrow().iter().position(|s| s.id == id) else { return };
        let manual = self.collections.borrow().len();
        app.global::<SmartUi>().set_open(false);
        app.global::<LibraryUi>().set_collection((1 + manual + pos) as i32);
        app.global::<Nav>().invoke_go(Page::Library);
        self.refresh_library();
    }
}

fn client_uri(g: &Game) -> Option<&'static str> {
    match g.source {
        GameSource::Steam => Some("steam://open/downloads"),
        GameSource::Epic if g.launch_uri.as_deref().is_some_and(|u| u.starts_with("heroic://")) => Some("heroic://"),
        GameSource::Epic if g.launch_uri.as_deref().is_some_and(|u| u.starts_with("com.epicgames")) => {
            Some("com.epicgames.launcher://store/library")
        }
        _ => None,
    }
}

fn client_label(g: &Game) -> String {
    match client_uri(g) {
        Some("steam://open/downloads") => tr!("Ouvrir Steam"),
        Some("heroic://") => tr!("Ouvrir Heroic"),
        Some(_) => tr!("Ouvrir Epic Games"),
        None => tr!("Client indisponible"),
    }
}

impl Controller {
    /// Export des statistiques (1.0.0) : fichier choisi par l'utilisateur.
    pub(crate) fn export_stats(&self, format: &str) {
        use halcyon_core::export;
        let (name, ext) = match format {
            "csv-games" => ("halcyon-temps-de-jeu.csv", "csv"),
            "csv-sessions" => ("halcyon-sessions.csv", "csv"),
            _ => ("halcyon-statistiques.json", "json"),
        };
        let Some(path) = crate::dialogs::save_file(name, ext) else { return };
        let sessions: Vec<export::SessionRow> = match self.db().sessions() {
            Ok(v) => v.into_iter().map(|s| export::SessionRow { game_id: s.game_id, started_at: s.started_at, duration_secs: s.duration_secs }).collect(),
            Err(e) => return self.toast(tr!("Lecture des sessions impossible : {e}", e = e), true),
        };
        let lib = self.lib.borrow();
        let data = match format {
            "csv-games" => export::games_csv(&lib.games),
            "csv-sessions" => export::sessions_csv(&sessions, &lib.games),
            _ => serde_json::to_string_pretty(&export::to_json(&sessions, &lib.games, now_unix(), env!("CARGO_PKG_VERSION"))).unwrap_or_default(),
        };
        drop(lib);
        // BOM UTF-8 pour les CSV : Excel affiche correctement les accents.
        let bytes = if ext == "csv" { [&[0xEF, 0xBB, 0xBF][..], data.as_bytes()].concat() } else { data.into_bytes() };
        match std::fs::write(&path, bytes) {
            Ok(()) => self.toast(tr!("Statistiques exportées : {path}", path = path.display()), false),
            Err(e) => self.toast(tr!("Export impossible : {e}", e = e), true),
        }
    }
}

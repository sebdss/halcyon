//! « Que jouer ? » : suggestions locales (aucun service distant), étagères
//! « En cours » et « Suggestions » de l'accueil.

use crate::controller::Controller;
use crate::models::card_for;
use crate::{AssistUi, SuggestRow};
use halcyon_core::model::GameId;
use halcyon_core::recommend::{self, Context, Suggestion, Suggestions};
use halcyon_storage::now_unix;
use slint::{ComponentHandle, ModelRc, VecModel};
use std::collections::HashMap;


impl Controller {
    pub(crate) fn suggestions(&self, minutes: u32, per_section: usize) -> Suggestions {
        let mut sessions: HashMap<GameId, Vec<u64>> = HashMap::new();
        let list = self.db().sessions().unwrap_or_default();
        for s in list {
            sessions.entry(s.game_id).or_default().push(s.duration_secs);
        }
        let ctx = Context { now: now_unix(), sessions, progress: self.progress.borrow().clone() };
        let lib = self.lib.borrow();
        recommend::suggest(&lib.games, &ctx, minutes, per_section)
    }

    fn suggest_rows(&self, list: &[Suggestion]) -> ModelRc<SuggestRow> {
        let lib = self.lib.borrow();
        let now = now_unix();
        let rows: Vec<SuggestRow> = list
            .iter()
            .filter_map(|s| lib.get(s.game).map(|g| (g, s)))
            .map(|(g, s)| SuggestRow { id: g.id as i32, reason: s.reason.clone().into(), card: card_for(g, &self.images, now) })
            .collect();
        ModelRc::new(VecModel::from(rows))
    }

    pub(crate) fn refresh_assistant(&self) {
        let Some(app) = self.app() else { return };
        let ui = app.global::<AssistUi>();
        let minutes = ui.get_minutes().max(0) as u32;
        let s = self.suggestions(minutes, 6);
        ui.set_for_time(self.suggest_rows(&s.for_time));
        ui.set_resume(self.suggest_rows(&s.resume));
        ui.set_almost_done(self.suggest_rows(&s.almost_done));
        ui.set_discover(self.suggest_rows(&s.discover));
        ui.set_rediscover(self.suggest_rows(&s.rediscover));
        ui.set_genres(s.top_genres.join(", ").into());
    }

    /// Ouvre un jeu tiré au hasard parmi les suggestions (pas de lancement automatique).
    pub(crate) fn surprise(&self) {
        let minutes = self.app().map(|a| a.global::<AssistUi>().get_minutes().max(0) as u32).unwrap_or(0);
        let s = self.suggestions(minutes, 10);
        let mut pool: Vec<GameId> = Vec::new();
        for g in s.for_time.iter().chain(&s.resume).chain(&s.discover).chain(&s.rediscover).chain(&s.almost_done) {
            if !pool.contains(&g.game) {
                pool.push(g.game);
            }
        }
        if pool.is_empty() {
            // Aucune suggestion fondée : un jeu installé au hasard, sans inventer de raison.
            let lib = self.lib.borrow();
            pool = lib.games.iter().filter(|g| g.installed && !g.hidden).map(|g| g.id).collect();
        }
        if pool.is_empty() {
            self.toast(tr!("Aucun jeu installé à proposer"), true);
            return;
        }
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
        // Mélange simple (xorshift) : pas besoin d'une source aléatoire cryptographique.
        let mut x = u64::from(nanos) ^ 0x9e37_79b9_7f4a_7c15;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        let id = pool[(x % pool.len() as u64) as usize];
        self.open_game(id);
    }
}

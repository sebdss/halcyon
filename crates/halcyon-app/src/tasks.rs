//! Centre de commandes : registre des tâches en cours et historique des évènements
//! (analyses, téléchargements, synchronisations, sauvegardes, notifications, erreurs).
//! Indépendant de l'interface : le contrôleur publie l'état dans `CommandUi`.

use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Running,
    Done,
    Failed,
    Info,
}

impl State {
    pub fn code(self) -> i32 {
        match self {
            Self::Running => 0,
            Self::Done => 1,
            Self::Failed => 2,
            Self::Info => 3,
        }
    }
}

/// Action proposée sur une tâche (bouton dans le centre de commandes).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    None,
    OpenPage(&'static str),
    OpenGame(i64),
    OpenFolder(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Task {
    pub id: u64,
    pub kind: &'static str,
    pub title: String,
    pub detail: String,
    /// 0..1, négatif si inconnue.
    pub progress: f32,
    pub state: State,
    pub started: i64,
    pub ended: Option<i64>,
    pub action: Action,
}

#[derive(Debug)]
pub struct TaskCenter {
    next: u64,
    active: Vec<Task>,
    history: VecDeque<Task>,
    limit: usize,
    /// Évènements terminés non encore vus (centre fermé).
    pub unread: usize,
}

impl Default for TaskCenter {
    fn default() -> Self {
        Self::new(80)
    }
}

impl TaskCenter {
    pub fn new(limit: usize) -> Self {
        Self { next: 1, active: Vec::new(), history: VecDeque::new(), limit, unread: 0 }
    }

    pub fn start(&mut self, kind: &'static str, title: impl Into<String>, now: i64) -> u64 {
        let id = self.next;
        self.next += 1;
        self.active.push(Task {
            id,
            kind,
            title: title.into(),
            detail: String::new(),
            progress: -1.0,
            state: State::Running,
            started: now,
            ended: None,
            action: Action::None,
        });
        id
    }

    /// Tâche unique d'un type (ex. une seule analyse à la fois) : réutilise l'existante.
    pub fn start_unique(&mut self, kind: &'static str, title: impl Into<String>, now: i64) -> u64 {
        if let Some(t) = self.active.iter_mut().find(|t| t.kind == kind) {
            t.title = title.into();
            return t.id;
        }
        self.start(kind, title, now)
    }

    pub fn find_kind(&self, kind: &str) -> Option<u64> {
        self.active.iter().find(|t| t.kind == kind).map(|t| t.id)
    }

    pub fn update(&mut self, id: u64, progress: Option<f32>, detail: Option<&str>) {
        if let Some(t) = self.active.iter_mut().find(|t| t.id == id) {
            if let Some(p) = progress {
                t.progress = p.clamp(-1.0, 1.0);
            }
            if let Some(d) = detail {
                t.detail = d.to_string();
            }
        }
    }

    pub fn finish(&mut self, id: u64, ok: bool, detail: impl Into<String>, action: Action, now: i64) {
        if let Some(pos) = self.active.iter().position(|t| t.id == id) {
            let mut t = self.active.remove(pos);
            t.state = if ok { State::Done } else { State::Failed };
            let d = detail.into();
            if !d.is_empty() {
                t.detail = d;
            }
            t.progress = 1.0;
            t.ended = Some(now);
            t.action = action;
            self.archive(t);
        }
    }

    /// Évènement ponctuel (notification, erreur) directement dans l'historique.
    pub fn event(&mut self, kind: &'static str, title: impl Into<String>, detail: impl Into<String>, error: bool, action: Action, now: i64) {
        let id = self.next;
        self.next += 1;
        let t = Task {
            id,
            kind,
            title: title.into(),
            detail: detail.into(),
            progress: -1.0,
            state: if error { State::Failed } else { State::Info },
            started: now,
            ended: Some(now),
            action,
        };
        self.archive(t);
    }

    fn archive(&mut self, t: Task) {
        // Un même message répété (ex. erreur réseau) n'encombre pas l'historique.
        if let Some(front) = self.history.front_mut() {
            if front.title == t.title && front.detail == t.detail && front.state == t.state {
                front.ended = t.ended;
                return;
            }
        }
        self.history.push_front(t);
        self.history.truncate(self.limit);
        self.unread += 1;
    }

    pub fn active(&self) -> &[Task] {
        &self.active
    }

    pub fn history(&self) -> impl Iterator<Item = &Task> {
        self.history.iter()
    }

    pub fn get(&self, id: u64) -> Option<&Task> {
        self.active.iter().chain(self.history.iter()).find(|t| t.id == id)
    }

    pub fn clear_history(&mut self) {
        self.history.clear();
        self.unread = 0;
    }

    pub fn errors(&self) -> usize {
        self.history.iter().filter(|t| t.state == State::Failed).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle() {
        let mut c = TaskCenter::new(3);
        let a = c.start("scan", "Analyse", 10);
        assert_eq!(c.start_unique("scan", "Analyse (Steam)", 11), a);
        c.update(a, Some(0.5), Some("Steam"));
        assert_eq!(c.active()[0].progress, 0.5);
        c.finish(a, true, "53 jeux", Action::OpenPage("library"), 20);
        assert!(c.active().is_empty());
        let t = c.history().next().unwrap();
        assert_eq!((t.state, t.detail.as_str(), t.ended), (State::Done, "53 jeux", Some(20)));
        assert_eq!(c.unread, 1);
    }

    #[test]
    fn events_dedup_and_limit() {
        let mut c = TaskCenter::new(3);
        c.event("net", "Réseau indisponible", "", true, Action::None, 1);
        c.event("net", "Réseau indisponible", "", true, Action::None, 2);
        assert_eq!(c.history().count(), 1);
        assert_eq!(c.errors(), 1);
        for i in 0..5 {
            c.event("info", format!("n{i}"), "", false, Action::None, 3);
        }
        assert_eq!(c.history().count(), 3);
        c.clear_history();
        assert_eq!((c.history().count(), c.unread), (0, 0));
    }
}

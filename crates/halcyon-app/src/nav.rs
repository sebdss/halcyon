//! Historique de navigation (précédent / suivant), indépendant de l'interface.

/// Entrée d'historique : page et, pour une fiche, l'identifiant du jeu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry<P> {
    pub page: P,
    pub game: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct History<P> {
    entries: Vec<Entry<P>>,
    index: usize,
    limit: usize,
}

impl<P: Copy + PartialEq> History<P> {
    pub fn new(start: P, limit: usize) -> Self {
        Self { entries: vec![Entry { page: start, game: None }], index: 0, limit: limit.max(2) }
    }

    pub fn current(&self) -> Entry<P> {
        self.entries[self.index]
    }

    /// Nouvelle visite : coupe l'historique « suivant », ignore les doublons consécutifs.
    pub fn push(&mut self, e: Entry<P>) {
        if self.entries[self.index] == e {
            return;
        }
        self.entries.truncate(self.index + 1);
        self.entries.push(e);
        if self.entries.len() > self.limit {
            let excess = self.entries.len() - self.limit;
            self.entries.drain(0..excess);
        }
        self.index = self.entries.len() - 1;
    }

    pub fn can_back(&self) -> bool {
        self.index > 0
    }

    pub fn can_forward(&self) -> bool {
        self.index + 1 < self.entries.len()
    }

    /// Déplacement de `delta` (-1 précédent, +1 suivant). `None` si impossible.
    pub fn go(&mut self, delta: i32) -> Option<Entry<P>> {
        let target = self.index as i64 + i64::from(delta);
        if target < 0 || target as usize >= self.entries.len() {
            return None;
        }
        self.index = target as usize;
        Some(self.entries[self.index])
    }

    /// Retire les fiches d'un jeu supprimé (jamais l'entrée courante).
    pub fn forget_game(&mut self, id: i64) {
        let current = self.current();
        self.entries.retain(|e| e.game != Some(id) || *e == current);
        self.entries.dedup();
        self.index = self.entries.iter().position(|e| *e == current).unwrap_or(self.entries.len() - 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(p: u8, g: Option<i64>) -> Entry<u8> {
        Entry { page: p, game: g }
    }

    #[test]
    fn back_forward_and_truncation() {
        let mut h = History::new(0u8, 50);
        assert!(!h.can_back());
        h.push(e(1, None));
        h.push(e(2, Some(7)));
        h.push(e(2, Some(7)));
        assert!(h.can_back() && !h.can_forward());
        assert_eq!(h.go(-1), Some(e(1, None)));
        assert_eq!(h.go(-1), Some(e(0, None)));
        assert_eq!(h.go(-1), None);
        assert_eq!(h.go(1), Some(e(1, None)));
        // Nouvelle visite depuis le milieu : la suite est oubliée.
        h.push(e(3, None));
        assert!(!h.can_forward());
        assert_eq!(h.go(-1), Some(e(1, None)));
    }

    #[test]
    fn limit_and_forget() {
        let mut h = History::new(0u8, 3);
        for i in 1..10 {
            h.push(e(i, None));
        }
        assert_eq!(h.go(-1), Some(e(8, None)));
        assert_eq!(h.go(-1), Some(e(7, None)));
        assert_eq!(h.go(-1), None);
        let mut h = History::new(0u8, 10);
        h.push(e(2, Some(5)));
        h.push(e(1, None));
        h.forget_game(5);
        assert_eq!(h.go(-1), Some(e(0, None)));
    }
}

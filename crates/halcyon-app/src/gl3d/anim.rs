//! Valeurs animées côté Rust (la scène 3D n'est pas animée par Slint) : approche exponentielle
//! vers la cible, réglée par le mouvement choisi. Une valeur arrivée ne demande plus d'image :
//! au repos, rien n'est redessiné.

/// Vitesse d'approche selon le réglage de mouvement (0 complet, 1 réduit, 2 aucun).
pub fn rate(motion: i32) -> f32 {
    match motion {
        0 => 9.0,
        1 => 18.0,
        _ => f32::INFINITY,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Smooth {
    pub value: f32,
    pub target: f32,
}

impl Smooth {
    pub fn new(v: f32) -> Self {
        Self { value: v, target: v }
    }

    pub fn set(&mut self, target: f32) {
        self.target = target;
    }

    /// Place directement (sans animation).
    pub fn snap(&mut self, v: f32) {
        self.value = v;
        self.target = v;
    }

    /// Avance de `dt` secondes ; renvoie `true` tant que la valeur bouge.
    pub fn step(&mut self, dt: f32, rate: f32) -> bool {
        let d = self.target - self.value;
        if d.abs() < 1e-3 || !rate.is_finite() {
            self.value = self.target;
            return false;
        }
        let k = 1.0 - (-dt.clamp(0.0, 0.1) * rate).exp();
        self.value += d * k;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converges_and_stops() {
        let mut s = Smooth::new(0.0);
        s.set(10.0);
        let mut frames = 0;
        while s.step(1.0 / 60.0, rate(0)) {
            frames += 1;
            assert!(frames < 200, "ne converge pas");
        }
        assert_eq!(s.value, 10.0);
        // Mouvement « aucun » : directement à la cible.
        s.set(3.0);
        assert!(!s.step(1.0 / 60.0, rate(2)));
        assert_eq!(s.value, 3.0);
    }
}

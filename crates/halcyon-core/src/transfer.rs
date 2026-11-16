//! Contrôle commun des téléchargements de Halcyon (1.0.0) : limite de débit, pause et
//! reprise, file d'attente (nombre de téléchargements simultanés) et compteurs pour le
//! gestionnaire de téléchargements.
//!
//! Concerne ce que Halcyon télécharge lui-même (Minecraft : versions, Java, bibliothèques,
//! loaders, mods, modpacks ; mises à jour de Halcyon). Les jeux Steam, Epic et GOG restent
//! téléchargés par leurs clients officiels.
//!
//! Sans dépendance : `std` seulement. Un réglage s'applique immédiatement à tous les
//! téléchargements en cours.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Seau à jetons : `rate` octets par seconde, rafale d'une demi-seconde au plus.
#[derive(Debug)]
struct Bucket {
    rate: u64,
    tokens: f64,
    last: Instant,
}

/// Contrôle partagé par tous les téléchargements.
#[derive(Debug)]
pub struct Transfers {
    bucket: Mutex<Bucket>,
    paused: Mutex<bool>,
    resumed: Condvar,
    paused_flag: AtomicBool,
    slots: Mutex<usize>,
    slot_freed: Condvar,
    max_slots: AtomicU64,
    /// Octets reçus depuis le démarrage (vitesse affichée par le gestionnaire).
    received: AtomicU64,
}

impl Default for Transfers {
    fn default() -> Self {
        Self::new()
    }
}

impl Transfers {
    pub fn new() -> Self {
        Self {
            bucket: Mutex::new(Bucket { rate: 0, tokens: 0.0, last: Instant::now() }),
            paused: Mutex::new(false),
            resumed: Condvar::new(),
            paused_flag: AtomicBool::new(false),
            slots: Mutex::new(0),
            slot_freed: Condvar::new(),
            max_slots: AtomicU64::new(2),
            received: AtomicU64::new(0),
        }
    }

    /// Limite en octets par seconde (0 : illimité).
    pub fn set_rate(&self, bytes_per_sec: u64) {
        let mut b = self.bucket.lock().unwrap_or_else(|e| e.into_inner());
        b.rate = bytes_per_sec;
        b.tokens = 0.0;
        b.last = Instant::now();
    }

    pub fn rate(&self) -> u64 {
        self.bucket.lock().unwrap_or_else(|e| e.into_inner()).rate
    }

    pub fn set_paused(&self, on: bool) {
        let mut p = self.paused.lock().unwrap_or_else(|e| e.into_inner());
        *p = on;
        self.paused_flag.store(on, Ordering::Relaxed);
        if !on {
            self.resumed.notify_all();
        }
    }

    pub fn is_paused(&self) -> bool {
        self.paused_flag.load(Ordering::Relaxed)
    }

    /// Téléchargements simultanés (1 à 8) ; les suivants attendent leur tour.
    pub fn set_max_concurrent(&self, n: usize) {
        self.max_slots.store(n.clamp(1, 8) as u64, Ordering::Relaxed);
        self.slot_freed.notify_all();
    }

    pub fn max_concurrent(&self) -> usize {
        self.max_slots.load(Ordering::Relaxed) as usize
    }

    pub fn received(&self) -> u64 {
        self.received.load(Ordering::Relaxed)
    }

    /// Attend une place dans la file (`cancel` interrompt l'attente : `None`).
    pub fn acquire(&self, cancel: &AtomicBool) -> Option<Slot<'_>> {
        let mut used = self.slots.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if cancel.load(Ordering::Relaxed) {
                return None;
            }
            if *used < self.max_concurrent() {
                *used += 1;
                return Some(Slot { owner: self });
            }
            used = self.slot_freed.wait_timeout(used, Duration::from_millis(250)).unwrap_or_else(|e| e.into_inner()).0;
        }
    }

    /// Nombre de téléchargements en cours (places occupées).
    pub fn active(&self) -> usize {
        *self.slots.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// À appeler après chaque bloc reçu : attend la fin d'une pause, puis respecte la limite
    /// de débit. Renvoie `false` si `cancel` est levé pendant l'attente.
    pub fn throttle(&self, n: u64, cancel: &AtomicBool) -> bool {
        self.received.fetch_add(n, Ordering::Relaxed);
        if self.is_paused() {
            let mut p = self.paused.lock().unwrap_or_else(|e| e.into_inner());
            while *p {
                if cancel.load(Ordering::Relaxed) {
                    return false;
                }
                p = self.resumed.wait_timeout(p, Duration::from_millis(250)).unwrap_or_else(|e| e.into_inner()).0;
            }
        }
        let wait = {
            let mut b = self.bucket.lock().unwrap_or_else(|e| e.into_inner());
            if b.rate == 0 {
                return !cancel.load(Ordering::Relaxed);
            }
            let now = Instant::now();
            let rate = b.rate as f64;
            b.tokens = (b.tokens + now.duration_since(b.last).as_secs_f64() * rate).min(rate * 0.5);
            b.last = now;
            b.tokens -= n as f64;
            if b.tokens < 0.0 {
                Duration::from_secs_f64(-b.tokens / rate)
            } else {
                Duration::ZERO
            }
        };
        // Attente découpée : une annulation ou un changement de limite est vu vite.
        let end = Instant::now() + wait;
        while Instant::now() < end {
            if cancel.load(Ordering::Relaxed) {
                return false;
            }
            std::thread::sleep((end - Instant::now()).min(Duration::from_millis(100)));
        }
        !cancel.load(Ordering::Relaxed)
    }
}

/// Place occupée dans la file, libérée à la destruction.
#[derive(Debug)]
pub struct Slot<'a> {
    owner: &'a Transfers,
}

impl Drop for Slot<'_> {
    fn drop(&mut self) {
        let mut used = self.owner.slots.lock().unwrap_or_else(|e| e.into_inner());
        *used = used.saturating_sub(1);
        self.owner.slot_freed.notify_all();
    }
}

/// Contrôle global du processus.
pub fn global() -> &'static Transfers {
    static G: OnceLock<Transfers> = OnceLock::new();
    G.get_or_init(Transfers::new)
}

/// Vitesse lissée à partir de deux relevés de `received()`.
pub fn speed(prev: (Instant, u64), now: (Instant, u64)) -> u64 {
    let dt = now.0.duration_since(prev.0).as_secs_f64();
    if dt <= 0.0 {
        0
    } else {
        (now.1.saturating_sub(prev.1) as f64 / dt) as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn rate_limit_is_respected() {
        let t = Transfers::new();
        t.set_rate(200_000);
        let cancel = AtomicBool::new(false);
        let start = Instant::now();
        // 300 Ko à 200 Ko/s, rafale initiale nulle : ~1,5 s.
        for _ in 0..30 {
            assert!(t.throttle(10_000, &cancel));
        }
        let e = start.elapsed().as_secs_f64();
        assert!((1.3..2.2).contains(&e), "{e}");
        assert_eq!(t.received(), 300_000);
        t.set_rate(0);
        let s = Instant::now();
        for _ in 0..100 {
            t.throttle(1_000_000, &cancel);
        }
        assert!(s.elapsed() < Duration::from_millis(50), "illimité : aucune attente");
    }

    #[test]
    fn pause_blocks_until_resumed_or_cancelled() {
        let t = Arc::new(Transfers::new());
        t.set_paused(true);
        let t2 = t.clone();
        let h = std::thread::spawn(move || {
            let c = AtomicBool::new(false);
            let s = Instant::now();
            t2.throttle(1, &c);
            s.elapsed()
        });
        std::thread::sleep(Duration::from_millis(300));
        t.set_paused(false);
        assert!(h.join().unwrap() >= Duration::from_millis(250));

        t.set_paused(true);
        let cancel = AtomicBool::new(true);
        assert!(!t.throttle(1, &cancel), "annulation pendant la pause");
    }

    #[test]
    fn queue_limits_concurrency() {
        let t = Arc::new(Transfers::new());
        t.set_max_concurrent(1);
        let c = AtomicBool::new(false);
        let first = t.acquire(&c).unwrap();
        assert_eq!(t.active(), 1);
        let t2 = t.clone();
        let h = std::thread::spawn(move || {
            let c = AtomicBool::new(false);
            let s = Instant::now();
            let _slot = t2.acquire(&c).unwrap();
            s.elapsed()
        });
        std::thread::sleep(Duration::from_millis(200));
        drop(first);
        assert!(h.join().unwrap() >= Duration::from_millis(150), "le second attend la fin du premier");
        assert_eq!(t.active(), 0);
        let _a = t.acquire(&c).unwrap();
        assert!(t.acquire(&AtomicBool::new(true)).is_none(), "attente annulée");
        t.set_max_concurrent(99);
        assert_eq!(t.max_concurrent(), 8);
    }

    #[test]
    fn speed_from_counters() {
        let a = Instant::now();
        assert_eq!(speed((a, 0), (a + Duration::from_secs(2), 1000)), 500);
        assert_eq!(speed((a, 0), (a, 10)), 0);
    }
}

//! Mesures internes : chronométrage des étapes (journal détaillé uniquement).

use std::time::Instant;

pub struct Stopwatch {
    name: &'static str,
    start: Instant,
    last: Instant,
    laps: Vec<(&'static str, u128)>,
}

impl Stopwatch {
    pub fn new(name: &'static str) -> Self {
        let now = Instant::now();
        Self { name, start: now, last: now, laps: Vec::new() }
    }

    pub fn lap(&mut self, label: &'static str) {
        let now = Instant::now();
        self.laps.push((label, now.duration_since(self.last).as_micros()));
        self.last = now;
    }

    pub fn finish(self) {
        if log::log_enabled!(log::Level::Debug) {
            let detail: Vec<String> = self.laps.iter().map(|(l, us)| format!("{l} {:.1} ms", *us as f64 / 1000.0)).collect();
            log::debug!("{} : {:.1} ms ({})", self.name, self.start.elapsed().as_micros() as f64 / 1000.0, detail.join(", "));
        }
    }
}

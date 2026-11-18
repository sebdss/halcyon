//! Agrégats des statistiques de jeu (logique pure, testable).
//!
//! Toutes les périodes sont calculées en heure locale (`tz` : décalage UTC en secondes).

use crate::util::{civil_from_days, days_from_civil, month_short};

const DAY: i64 = 86_400;

/// Une barre d'histogramme.
#[derive(Clone, Debug, PartialEq)]
pub struct Bucket {
    pub label: String,
    /// Début de la période (horodatage Unix local ramené en UTC).
    pub start: i64,
    pub value: u64,
}

fn local_day(t: i64, tz: i64) -> i64 {
    (t + tz).div_euclid(DAY)
}

/// Lundi de la semaine contenant le jour `day` (1970-01-01 était un jeudi).
fn monday(day: i64) -> i64 {
    day - (day + 3).rem_euclid(7)
}

/// `n` dernières semaines (lundi → dimanche), la plus ancienne en premier.
/// `items` : (horodatage, valeur).
pub fn weekly(items: impl IntoIterator<Item = (i64, u64)>, now: i64, tz: i64, n: usize) -> Vec<Bucket> {
    let this_week = monday(local_day(now, tz));
    let mut out: Vec<Bucket> = (0..n)
        .rev()
        .map(|i| {
            let start = this_week - 7 * i as i64;
            let (_, m, d) = civil_from_days(start);
            Bucket { label: trc!("date", "{day} {month}", day = d, month = month_short(m)), start: start * DAY - tz, value: 0 }
        })
        .collect();
    for (t, v) in items {
        let w = monday(local_day(t, tz));
        let idx = (this_week - w) / 7;
        if (0..n as i64).contains(&idx) && w <= this_week {
            out[n - 1 - idx as usize].value += v;
        }
    }
    out
}

/// `n` derniers mois civils, le plus ancien en premier.
pub fn monthly(items: impl IntoIterator<Item = (i64, u64)>, now: i64, tz: i64, n: usize) -> Vec<Bucket> {
    let (y, m, _) = civil_from_days(local_day(now, tz));
    let current = y * 12 + (m as i64 - 1);
    let mut out: Vec<Bucket> = (0..n)
        .rev()
        .map(|i| {
            let idx = current - i as i64;
            let (yy, mm) = (idx.div_euclid(12), (idx.rem_euclid(12) + 1) as u32);
            let label = if mm == 1 || i == 0 || i == n - 1 {
                trc!("date", "{month} {year}", month = month_short(mm), year = format!("{:02}", yy % 100))
            } else {
                month_short(mm)
            };
            Bucket { label, start: days_from_civil(yy, mm, 1) * DAY - tz, value: 0 }
        })
        .collect();
    for (t, v) in items {
        let (yy, mm, _) = civil_from_days(local_day(t, tz));
        let idx = current - (yy * 12 + (mm as i64 - 1));
        if (0..n as i64).contains(&idx) {
            out[n - 1 - idx as usize].value += v;
        }
    }
    out
}

/// Ratio de chaque barre par rapport à la plus haute (pour l'affichage).
pub fn ratios(buckets: &[Bucket]) -> Vec<f32> {
    let max = buckets.iter().map(|b| b.value).max().unwrap_or(0).max(1);
    buckets.iter().map(|b| b.value as f32 / max as f32).collect()
}

/// Série de jours consécutifs avec au moins une session, se terminant aujourd'hui ou hier.
pub fn streak(session_starts: impl IntoIterator<Item = i64>, now: i64, tz: i64) -> u32 {
    let mut days: Vec<i64> = session_starts.into_iter().map(|t| local_day(t, tz)).collect();
    days.sort_unstable();
    days.dedup();
    let today = local_day(now, tz);
    let Some(&last) = days.last() else { return 0 };
    if last < today - 1 {
        return 0;
    }
    let mut n = 1;
    for w in days.windows(2).rev() {
        if w[1] - w[0] == 1 {
            n += 1;
        } else {
            break;
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mardi 3 novembre 2026, 12:00 UTC.
    const NOW: i64 = 1_793_707_200;

    #[test]
    fn weeks_start_on_monday_in_local_time() {
        let (y, m, d) = civil_from_days(NOW / DAY);
        assert_eq!((y, m, d), (2026, 11, 3));
        let b = weekly([(NOW, 60), (NOW - 2 * DAY, 30), (NOW - 8 * DAY, 10), (NOW - 400 * DAY, 99)], NOW, 3600, 4);
        assert_eq!(b.len(), 4);
        assert_eq!(b[3].label, "2 nov.");
        // Dimanche 1er novembre → semaine précédente.
        assert_eq!(b.iter().map(|x| x.value).collect::<Vec<_>>(), [0, 0, 40, 60]);
    }

    #[test]
    fn months_and_streaks() {
        let b = monthly([(NOW, 5), (NOW - 40 * DAY, 2)], NOW, 0, 3);
        assert_eq!(b.iter().map(|x| x.label.as_str()).collect::<Vec<_>>(), ["sept. 26", "oct.", "nov. 26"]);
        assert_eq!(b.iter().map(|x| x.value).collect::<Vec<_>>(), [2, 0, 5]);
        assert_eq!(ratios(&b), [0.4, 0.0, 1.0]);
        assert_eq!(streak([NOW, NOW - DAY, NOW - 2 * DAY, NOW - 5 * DAY], NOW, 0), 3);
        assert_eq!(streak([NOW - 3 * DAY], NOW, 0), 0);
    }
}

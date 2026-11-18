//! Export des statistiques (1.0.0) : CSV (tableurs) et JSON (outils, archives).
//!
//! Deux jeux de données : temps de jeu par jeu (valeurs de la bibliothèque, telles
//! qu'affichées) et historique des sessions suivies par Halcyon. Dates en ISO 8601 UTC.
//! Aucune donnée n'est inventée : un champ inconnu reste vide (CSV) ou `null` (JSON).

use crate::util::civil_from_days;
use crate::{Game, GameId};
use std::collections::HashMap;

/// Session de jeu à exporter (miroir de `halcyon_storage::Session`, sans dépendance).
#[derive(Clone, Debug, PartialEq)]
pub struct SessionRow {
    pub game_id: GameId,
    pub started_at: i64,
    pub duration_secs: u64,
}

/// « 2026-11-16T10:20:00Z ».
pub fn iso(t: i64) -> String {
    let days = t.div_euclid(86_400);
    let s = t.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", s / 3600, s / 60 % 60, s % 60)
}

/// Champ CSV : guillemets si nécessaire, et neutralisation des formules de tableur
/// (`=`, `+`, `-`, `@` en tête : injection CSV).
pub fn csv_field(v: &str) -> String {
    let v = if v.starts_with(['=', '+', '-', '@', '\t', '\r']) { format!("'{v}") } else { v.to_string() };
    if v.contains([',', '"', '\n', '\r', ';']) {
        format!("\"{}\"", v.replace('"', "\"\""))
    } else {
        v
    }
}

fn line(fields: &[String]) -> String {
    let mut s = fields.iter().map(|f| csv_field(f)).collect::<Vec<_>>().join(",");
    s.push_str("\r\n");
    s
}

pub fn games_csv(games: &[Game]) -> String {
    let mut out = line(&[
        "id".to_string(),
        trc!("csv", "nom"),
        trc!("csv", "source"),
        trc!("csv", "installé"),
        trc!("csv", "favori"),
        trc!("csv", "statut"),
        trc!("csv", "temps_de_jeu_s"),
        trc!("csv", "temps_de_jeu_h"),
        trc!("csv", "dernier_lancement"),
    ]);
    for g in games.iter().filter(|g| !g.hidden) {
        out.push_str(&line(&[
            g.id.to_string(),
            g.name.clone(),
            g.source.label().to_string(),
            if g.installed { tr!("oui") } else { tr!("non") },
            if g.favorite { tr!("oui") } else { tr!("non") },
            g.status.label().to_string(),
            g.playtime_secs.to_string(),
            format!("{:.2}", g.playtime_secs as f64 / 3600.0),
            g.last_played.map(iso).unwrap_or_default(),
        ]));
    }
    out
}

pub fn sessions_csv(sessions: &[SessionRow], games: &[Game]) -> String {
    let names: HashMap<GameId, &str> = games.iter().map(|g| (g.id, g.name.as_str())).collect();
    let mut out = line(&[
        trc!("csv", "id_jeu"),
        trc!("csv", "jeu"),
        trc!("csv", "début"),
        trc!("csv", "fin"),
        trc!("csv", "durée_s"),
        trc!("csv", "durée_min"),
    ]);
    for s in sessions {
        out.push_str(&line(&[
            s.game_id.to_string(),
            names.get(&s.game_id).copied().unwrap_or("").to_string(),
            iso(s.started_at),
            iso(s.started_at + s.duration_secs as i64),
            s.duration_secs.to_string(),
            format!("{:.1}", s.duration_secs as f64 / 60.0),
        ]));
    }
    out
}

pub fn to_json(sessions: &[SessionRow], games: &[Game], exported_at: i64, app_version: &str) -> serde_json::Value {
    let games_json: Vec<serde_json::Value> = games
        .iter()
        .filter(|g| !g.hidden)
        .map(|g| {
            serde_json::json!({
                "id": g.id, "name": g.name, "source": g.source.as_str(), "installed": g.installed,
                "favorite": g.favorite, "status": g.status.label(), "playtime_secs": g.playtime_secs,
                "last_played": g.last_played.map(iso),
            })
        })
        .collect();
    let sessions_json: Vec<serde_json::Value> = sessions
        .iter()
        .map(|s| serde_json::json!({ "game_id": s.game_id, "started_at": iso(s.started_at), "duration_secs": s.duration_secs }))
        .collect();
    serde_json::json!({
        "format": "halcyon-stats", "version": 1, "app_version": app_version, "exported_at": iso(exported_at),
        "games": games_json, "sessions": sessions_json,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GameSource;

    fn game(id: GameId, name: &str) -> Game {
        let mut g = Game::blank(id, name, GameSource::Steam);
        g.playtime_secs = 5400;
        g.last_played = Some(1_763_288_400);
        g
    }

    #[test]
    fn iso_dates() {
        assert_eq!(iso(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso(1_763_288_400), "2025-11-16T10:20:00Z");
    }

    #[test]
    fn csv_escapes_and_neutralizes_formulas() {
        assert_eq!(csv_field("Halo, Reach"), "\"Halo, Reach\"");
        assert_eq!(csv_field("Say \"hi\""), "\"Say \"\"hi\"\"\"");
        assert_eq!(csv_field("=HYPERLINK(\"x\")"), "\"'=HYPERLINK(\"\"x\"\")\"");
        assert_eq!(csv_field("-1"), "'-1");
        let mut hidden = game(3, "Caché");
        hidden.hidden = true;
        let csv = games_csv(&[game(1, "Doom, Eternal"), hidden]);
        let lines: Vec<&str> = csv.split("\r\n").collect();
        assert_eq!(lines.len(), 3, "en-tête, une ligne, fin");
        assert!(lines[1].starts_with("1,\"Doom, Eternal\",Steam,oui,non,"));
        assert!(lines[1].contains(",5400,1.50,2025-11-16T10:20:00Z"));
    }

    #[test]
    fn sessions_and_json() {
        let games = [game(1, "A")];
        let s = [SessionRow { game_id: 1, started_at: 0, duration_secs: 90 }, SessionRow { game_id: 9, started_at: 60, duration_secs: 30 }];
        let csv = sessions_csv(&s, &games);
        assert!(csv.contains("1,A,1970-01-01T00:00:00Z,1970-01-01T00:01:30Z,90,1.5\r\n"));
        assert!(csv.contains("9,,1970-01-01T00:01:00Z"), "jeu retiré : nom vide");
        let j = to_json(&s, &games, 0, "1.0.0");
        assert_eq!(j["games"][0]["playtime_secs"], 5400);
        assert_eq!(j["sessions"].as_array().unwrap().len(), 2);
        assert_eq!(j["games"][0]["source"], "steam");
    }
}

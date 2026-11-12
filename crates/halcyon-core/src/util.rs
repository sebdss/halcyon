//! Utilitaires purs : normalisation de texte, dates, durées.

use std::path::Path;

/// Remplace les lettres accentuées latines courantes par leur équivalent ASCII.
pub fn fold_char(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
        'ç' => 'c',
        'è' | 'é' | 'ê' | 'ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' => 'i',
        'ñ' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' => 'o',
        'ù' | 'ú' | 'û' | 'ü' => 'u',
        'ý' | 'ÿ' => 'y',
        _ => c,
    }
}

/// Clé de recherche : minuscules, sans accents, ponctuation remplacée par des espaces.
/// « Assassin's Creed® Valhalla » → « assassin s creed valhalla ».
pub fn search_key(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut last_space = true;
    for c in name.chars().flat_map(char::to_lowercase).map(fold_char) {
        if c.is_alphanumeric() {
            out.push(c);
            last_space = false;
        } else if !last_space {
            out.push(' ');
            last_space = true;
        }
    }
    if out.ends_with(' ') {
        out.pop();
    }
    out
}

/// Clé de tri alphabétique : ignore l'article anglais initial (« The Witcher 3 » → « witcher 3 »)
/// et complète les nombres pour un tri naturel (« Halo 2 » avant « Halo 10 »).
pub fn sort_key(name: &str) -> String {
    let base = search_key(name);
    let base = base.strip_prefix("the ").unwrap_or(&base);
    let mut out = String::with_capacity(base.len() + 8);
    let mut digits = String::new();
    for c in base.chars() {
        if c.is_ascii_digit() {
            digits.push(c);
        } else {
            flush_digits(&mut out, &mut digits);
            out.push(c);
        }
    }
    flush_digits(&mut out, &mut digits);
    out
}

fn flush_digits(out: &mut String, digits: &mut String) {
    if !digits.is_empty() {
        for _ in digits.len()..8 {
            out.push('0');
        }
        out.push_str(digits);
        digits.clear();
    }
}

/// Clé de déduplication d'un chemin (insensible à la casse et aux séparateurs).
pub fn path_key(path: &Path) -> String {
    let s = path.to_string_lossy().replace('\\', "/").to_lowercase();
    s.trim_end_matches('/').to_string()
}

/// Hash FNV-1a 64 bits, stable entre les exécutions (contrairement à `DefaultHasher`).
pub fn stable_hash(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Deux teintes (0–360) dérivées du nom, pour les jaquettes de remplacement.
pub fn placeholder_hues(name: &str) -> (f32, f32) {
    let h = stable_hash(name);
    let a = (h % 360) as f32;
    let b = (a + 35.0 + ((h >> 16) % 50) as f32) % 360.0;
    (a, b)
}

/// Initiales affichées sur une jaquette de remplacement (« Hollow Knight » → « HK »).
pub fn initials(name: &str) -> String {
    let words: Vec<&str> = name.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    let mut s: String = words.iter().take(2).filter_map(|w| w.chars().next()).flat_map(char::to_uppercase).collect();
    if s.is_empty() {
        s.push('?');
    }
    s
}

/// Durée de jeu lisible : « 42 min », « 3 h 05 », « 1 250 h ».
pub fn format_playtime(secs: u64) -> String {
    let minutes = secs / 60;
    if minutes == 0 {
        return if secs == 0 { "Jamais joué".into() } else { "< 1 min".into() };
    }
    if minutes < 60 {
        return format!("{minutes} min");
    }
    let hours = minutes / 60;
    if hours < 100 {
        format!("{} h {:02}", hours, minutes % 60)
    } else {
        format!("{} h", group_thousands(hours))
    }
}

fn group_thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push('\u{202F}');
        }
        out.push(c);
    }
    out
}

/// Date civile (année, mois, jour) depuis un nombre de jours depuis 1970-01-01.
/// Algorithme de Howard Hinnant (exact sur le calendrier grégorien proleptique).
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

const MONTHS_FR: [&str; 12] =
    ["janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc."];

/// « 3 nov. 2026 ».
pub fn format_date(unix: i64) -> String {
    let (y, m, d) = civil_from_days(unix.div_euclid(86_400));
    format!("{d} {} {y}", MONTHS_FR[(m - 1) as usize])
}

/// Date relative : « Aujourd'hui », « Hier », « Il y a 5 jours », sinon date complète.
pub fn format_relative(unix: i64, now: i64) -> String {
    let days = now.div_euclid(86_400) - unix.div_euclid(86_400);
    match days {
        i64::MIN..=-1 => format_date(unix),
        0 => "Aujourd'hui".into(),
        1 => "Hier".into(),
        2..=6 => format!("Il y a {days} jours"),
        _ => format_date(unix),
    }
}

/// Taille disque lisible : « 48,2 Go ».
pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["o", "Ko", "Mo", "Go", "To"];
    let mut v = bytes as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{bytes} o")
    } else {
        format!("{:.1} {}", v, UNITS[u]).replace('.', ",")
    }
}

/// Jours depuis 1970-01-01 pour une date civile (inverse de [`civil_from_days`]).
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let m = m as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Analyse « AAAA-MM-JJ HH:MM:SS » (UTC) en horodatage Unix.
pub fn parse_datetime(s: &str) -> Option<i64> {
    let s = s.trim();
    let (date, time) = s.split_once([' ', 'T']).unwrap_or((s, "00:00:00"));
    let mut d = date.split('-').map(|p| p.parse::<i64>());
    let (y, m, day) = (d.next()?.ok()?, d.next()?.ok()?, d.next()?.ok()?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&day) {
        return None;
    }
    let mut t = time.trim_end_matches('Z').split(':').map(|p| p.split('.').next().unwrap_or("0").parse::<i64>());
    let h = t.next().and_then(|v| v.ok()).unwrap_or(0);
    let mi = t.next().and_then(|v| v.ok()).unwrap_or(0);
    let se = t.next().and_then(|v| v.ok()).unwrap_or(0);
    Some(days_from_civil(y, m as u32, day as u32) * 86_400 + h * 3600 + mi * 60 + se)
}

/// Mois « nov. 2026 » d'un horodatage.
pub fn format_month(unix: i64) -> String {
    let (y, m, _) = civil_from_days(unix.div_euclid(86_400));
    format!("{} {y}", MONTHS_FR[(m - 1) as usize])
}

/// Décode les entités HTML courantes et retire les balises (descriptions des boutiques).
pub fn strip_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&quot;", "\"").replace("&#39;", "'").replace("&apos;", "'").replace("&lt;", "<").replace("&gt;", ">")
        .replace("&nbsp;", " ").replace("&amp;", "&").trim().to_string()
}

/// Compare deux versions « 1.2.3 » (préfixe « v » toléré).
pub fn version_newer(candidate: &str, current: &str) -> bool {
    let parse = |v: &str| -> Vec<u64> {
        v.trim().trim_start_matches(['v', 'V']).split(['.', '-']).map_while(|p| p.parse().ok()).collect()
    };
    parse(candidate) > parse(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_key_folds_accents_and_punctuation() {
        assert_eq!(search_key("Assassin's Creed® Valhalla"), "assassin s creed valhalla");
        assert_eq!(search_key("Pokémon  Écarlate!"), "pokemon ecarlate");
    }

    #[test]
    fn sort_key_is_natural_and_ignores_article() {
        assert!(sort_key("Halo 2") < sort_key("Halo 10"));
        assert_eq!(sort_key("The Witcher 3"), sort_key("Witcher 3"));
    }

    #[test]
    fn dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_760), (2026, 11, 3));
        assert_eq!(format_date(1_793_664_000), "3 nov. 2026");
        assert_eq!(format_relative(1_793_664_000, 1_793_664_000 + 86_400), "Hier");
    }

    #[test]
    fn playtime() {
        assert_eq!(format_playtime(0), "Jamais joué");
        assert_eq!(format_playtime(42 * 60), "42 min");
        assert_eq!(format_playtime(3 * 3600 + 5 * 60), "3 h 05");
        assert_eq!(format_playtime(1250 * 3600), "1\u{202F}250 h");
    }

    #[test]
    fn datetime_parsing() {
        assert_eq!(parse_datetime("2026-11-03 00:00:00"), Some(1_793_664_000));
        assert_eq!(parse_datetime("2016-03-12 17:47:29"), Some(1_457_804_849));
        assert_eq!(parse_datetime("2026-11-03T01:00:00Z"), Some(1_793_667_600));
        assert_eq!(parse_datetime("garbage"), None);
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(format_month(1_793_664_000), "nov. 2026");
    }

    #[test]
    fn html_and_versions() {
        assert_eq!(strip_html("<b>Rock</b> &amp; roll&nbsp;!"), "Rock & roll !");
        assert!(version_newer("v0.2.0", "0.1.9"));
        assert!(!version_newer("0.1.0", "0.1.0"));
        assert!(version_newer("1.10.0", "1.9.3"));
    }

    #[test]
    fn misc() {
        assert_eq!(initials("Hollow Knight"), "HK");
        assert_eq!(initials("  "), "?");
        assert_eq!(path_key(Path::new("C:\\Games\\Foo\\")), "c:/games/foo");
        assert_eq!(format_size(1536), "1,5 Ko");
    }
}

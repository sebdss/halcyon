//! Recherche avancée : texte approximatif et filtres par champ.
//!
//! Syntaxe (insensible à la casse et aux accents) :
//! - mots libres : cherchés dans le nom, avec tolérance aux fautes de frappe
//!   (`witchr` trouve « The Witcher ») et aux sigles (`gta` → « Grand Theft Auto ») ;
//! - `champ:valeur` ou `champ:"valeur avec espaces"` ; un `-` devant inverse le filtre.
//!
//! | Champ | Exemples |
//! |---|---|
//! | `dev:` `développeur:` | `dev:fromsoftware` |
//! | `éditeur:` `pub:` | `éditeur:"bandai namco"` |
//! | `genre:` | `genre:rpg` |
//! | `plateforme:` `source:` | `plateforme:snes`, `source:steam` |
//! | `tag:` | `tag:multijoueur` |
//! | `installé:` | `installé:non` |
//! | `favori:` | `favori:oui` |
//! | `statut:` | `statut:terminé`, `statut:"en cours"` |
//! | `temps:` | `temps:>10h`, `temps:<30m`, `temps:0` |
//! | `succès:` | `succès:complet`, `succès:manquants`, `succès:>80`, `succès:aucun` |
//! | `maj:` | `maj:oui` (mise à jour en attente) |

use crate::achievements::Progress;
use crate::model::{Game, GameId, GameSource, PlayStatus};
use crate::util;
use std::collections::HashMap;

/// Données externes au jeu nécessaires à certains filtres.
#[derive(Clone, Debug, Default)]
pub struct QueryContext {
    /// Horodatage Unix courant (règles « joué cette semaine »…).
    pub now: i64,
    pub progress: HashMap<GameId, Progress>,
    /// Identifiant de plateforme émulée → nom affiché (« snes » → « Super Nintendo »).
    pub platform_names: HashMap<String, String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cmp {
    Lt,
    Le,
    Eq,
    Ge,
    Gt,
}

impl Cmp {
    fn test(self, a: u64, b: u64) -> bool {
        match self {
            Self::Lt => a < b,
            Self::Le => a <= b,
            Self::Eq => a == b,
            Self::Ge => a >= b,
            Self::Gt => a > b,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum AchFilter {
    /// Le jeu a des succès connus.
    Any,
    /// Aucun succès connu.
    None,
    Complete,
    Missing,
    Percent(Cmp, u64),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Filter {
    Developer(String),
    Publisher(String),
    Genre(String),
    Platform(String),
    Tag(String),
    Installed(bool),
    Favorite(bool),
    Status(PlayStatus),
    Playtime(Cmp, u64),
    Achievements(AchFilter),
    NeedsUpdate(bool),
    Not(Box<Filter>),
}

/// Requête analysée.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SearchQuery {
    /// Mots libres, normalisés.
    pub terms: Vec<String>,
    pub filters: Vec<Filter>,
    /// Filtres non reconnus (affichés comme avertissement, jamais ignorés en silence).
    pub errors: Vec<String>,
}

/// Qualité de la correspondance textuelle (sert au classement).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MatchTier {
    Fuzzy = 1,
    Direct = 2,
}

impl SearchQuery {
    pub fn parse(input: &str) -> Self {
        let mut q = SearchQuery::default();
        for token in tokenize(input) {
            let (negate, body) = match token.strip_prefix('-') {
                Some(rest) if rest.contains(':') => (true, rest),
                _ => (false, token.as_str()),
            };
            if let Some((field, value)) = body.split_once(':') {
                let field_key = util::search_key(field).replace(' ', "");
                if is_field(&field_key) {
                    match parse_filter(&field_key, value.trim_matches('"')) {
                        Some(f) => q.filters.push(if negate { Filter::Not(Box::new(f)) } else { f }),
                        None => q.errors.push(format!("{field}:{value}")),
                    }
                    continue;
                }
            }
            let key = util::search_key(&token);
            q.terms.extend(key.split(' ').filter(|t| !t.is_empty()).map(str::to_string));
        }
        q
    }

    pub fn is_empty(&self) -> bool {
        self.terms.is_empty() && self.filters.is_empty()
    }

    /// `Some(tier)` si le jeu correspond. Sans mots libres, toute correspondance est directe.
    pub fn matches(&self, g: &Game, ctx: &QueryContext) -> Option<MatchTier> {
        if !self.filters.iter().all(|f| filter_matches(f, g, ctx)) {
            return None;
        }
        text_match(&self.terms, g)
    }
}

/// Correspondance des mots libres avec le nom du jeu.
pub fn text_match(terms: &[String], g: &Game) -> Option<MatchTier> {
    if terms.is_empty() {
        return Some(MatchTier::Direct);
    }
    let words: Vec<&str> = g.search_key.split(' ').filter(|w| !w.is_empty()).collect();
    let mut tier = MatchTier::Direct;
    for t in terms {
        if g.search_key.contains(t.as_str()) || is_acronym(t, &words) {
            continue;
        }
        if fuzzy_word(t, &words) {
            tier = MatchTier::Fuzzy;
            continue;
        }
        return None;
    }
    Some(tier)
}

fn is_acronym(term: &str, words: &[&str]) -> bool {
    term.len() >= 2 && words.len() >= term.len() && {
        let initials: String = words.iter().filter_map(|w| w.chars().next()).collect();
        initials.contains(term)
    }
}

/// Un mot du nom est-il proche du terme (fautes de frappe, mot incomplet) ?
fn fuzzy_word(term: &str, words: &[&str]) -> bool {
    let mut t = ['\0'; MAX_WORD];
    let n = fill(term, &mut t);
    if !(3..MAX_WORD).contains(&n) {
        return false;
    }
    let max = if n <= 5 { 1 } else { 2 };
    let mut w = ['\0'; MAX_WORD];
    words.iter().any(|word| {
        let m = fill(word, &mut w);
        if m >= MAX_WORD || m + 2 < n.saturating_sub(1) {
            return false;
        }
        let (full, prefix) = dl(&t[..n], &w[..m]);
        full <= max || prefix <= 1
    })
}

const MAX_WORD: usize = 40;

fn fill(s: &str, out: &mut [char; MAX_WORD]) -> usize {
    let mut n = 0;
    for c in s.chars() {
        if n == MAX_WORD {
            return MAX_WORD;
        }
        out[n] = c;
        n += 1;
    }
    n
}

/// Distance de Damerau-Levenshtein entre `a` et `b`, et meilleure distance entre `a`
/// et un début de `b` de longueur proche (mot en cours de frappe).
fn dl(a: &[char], b: &[char]) -> (usize, usize) {
    let (n, m) = (a.len(), b.len());
    let mut rows = [[0usize; MAX_WORD + 1]; 3];
    for (j, v) in rows[0].iter_mut().enumerate().take(m + 1) {
        *v = j;
    }
    for i in 1..=n {
        let (cur, prev, prev2) = (i % 3, (i + 2) % 3, (i + 1) % 3);
        rows[cur][0] = i;
        for j in 1..=m {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut v = (rows[prev][j] + 1).min(rows[cur][j - 1] + 1).min(rows[prev][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                v = v.min(rows[prev2][j - 2] + 1);
            }
            rows[cur][j] = v;
        }
    }
    let last = &rows[n % 3];
    let prefix = (n.saturating_sub(1)..=n + 1).filter(|&l| l >= 3 && l < m).map(|l| last[l]).min().unwrap_or(usize::MAX);
    (last[m], prefix)
}

/// Distance de Damerau-Levenshtein (transpositions adjacentes).
pub fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().take(MAX_WORD).collect();
    let b: Vec<char> = b.chars().take(MAX_WORD).collect();
    dl(&a, &b).0
}

fn tokenize(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for c in input.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                cur.push(c);
            }
            c if c.is_whitespace() && !quoted => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Champs reconnus. Les mots-clés existent dans chaque langue de l'interface (forme sans
/// accents, comme après `util::search_key`) : l'aide de la recherche, traduite, cite ceux de
/// la langue active.
const FIELDS: &[&str] = &[
    // français
    "dev", "developpeur", "studio", "editeur", "pub", "genre", "genero", "genere", "plateforme", "console", "source", "tag", "etiquette",
    "installe", "favori", "fav", "statut", "temps", "succes", "maj",
    // anglais
    "developer", "publisher", "platform", "installed", "favorite", "favourite", "status", "playtime", "time",
    "achievements", "update",
    // allemand
    "entwickler", "herausgeber", "plattform", "installiert", "favorit", "spielzeit", "erfolge", "aktualisierung",
    // espagnol
    "desarrollador", "editor", "plataforma", "etiqueta", "instalado", "favorito", "estado", "tiempo", "logros",
    "actualizacion",
    // italien
    "sviluppatore", "editore", "piattaforma", "etichetta", "installato", "preferito", "stato", "tempo", "obiettivi",
    "aggiornamento",
    // portugais
    "programador", "editora", "conquistas", "atualizacao",
];

fn is_field(f: &str) -> bool {
    FIELDS.contains(&f)
}

fn yes_no(v: &str) -> Option<bool> {
    match util::search_key(v).as_str() {
        "oui" | "yes" | "1" | "vrai" | "true" | "o" | "y" | "ja" | "j" | "si" | "s" | "sim" => Some(true),
        "non" | "no" | "0" | "faux" | "false" | "n" | "nein" | "nao" => Some(false),
        _ => None,
    }
}

fn parse_cmp(v: &str) -> (Cmp, &str) {
    for (p, c) in [(">=", Cmp::Ge), ("<=", Cmp::Le), (">", Cmp::Gt), ("<", Cmp::Lt), ("=", Cmp::Eq)] {
        if let Some(rest) = v.strip_prefix(p) {
            return (c, rest.trim());
        }
    }
    (Cmp::Eq, v.trim())
}

/// « 10h », « 90m », « 1h30 », « 2 » (heures) → secondes.
pub fn parse_duration(v: &str) -> Option<u64> {
    let v = v.trim().to_lowercase().replace(' ', "");
    if v.is_empty() {
        return None;
    }
    if let Some((h, m)) = v.split_once('h') {
        let h: f64 = h.replace(',', ".").parse().ok()?;
        let m: u64 = if m.is_empty() { 0 } else { m.trim_end_matches("min").trim_end_matches('m').parse().ok()? };
        return Some((h * 3600.0) as u64 + m * 60);
    }
    if let Some(m) = v.strip_suffix("min").or_else(|| v.strip_suffix('m')) {
        return Some(m.parse::<u64>().ok()? * 60);
    }
    let h: f64 = v.replace(',', ".").parse().ok()?;
    Some((h * 3600.0) as u64)
}

fn parse_filter(field: &str, value: &str) -> Option<Filter> {
    let text = || Some(util::search_key(value)).filter(|v| !v.is_empty());
    Some(match field {
        "dev" | "developpeur" | "developer" | "studio" | "entwickler" | "desarrollador" | "sviluppatore" | "programador" => Filter::Developer(text()?),
        "editeur" | "publisher" | "pub" | "herausgeber" | "editor" | "editore" | "editora" => Filter::Publisher(text()?),
        "genre" | "genero" | "genere" => Filter::Genre(text()?),
        "plateforme" | "platform" | "console" | "source" | "plattform" | "plataforma" | "piattaforma" => Filter::Platform(text()?),
        "tag" | "etiquette" | "etiqueta" | "etichetta" => Filter::Tag(text()?),
        "installe" | "installed" | "installiert" | "instalado" | "installato" => Filter::Installed(yes_no(value)?),
        "favori" | "fav" | "favorite" | "favourite" | "favorit" | "favorito" | "preferito" => Filter::Favorite(yes_no(value)?),
        "maj" | "update" | "aktualisierung" | "actualizacion" | "aggiornamento" | "atualizacao" => Filter::NeedsUpdate(yes_no(value)?),
        "statut" | "status" | "estado" | "stato" => {
            let v = util::search_key(value).replace(' ', "");
            Filter::Status(match v.as_str() {
                "termine" | "fini" | "completed" | "finished" | "done" | "terminato" | "abgeschlossen" | "beendet" | "completado" | "terminado" | "completato" | "concluido" => PlayStatus::Completed,
                "encours" | "playing" | "laufend" | "spielend" | "amspielen" | "jugando" | "incorso" | "ajogar" | "emcurso" => PlayStatus::Playing,
                "ajouer" | "backlog" | "afaire" | "geplant" | "pendiente" | "dagiocare" | "pendente" | "porjogar" => PlayStatus::Backlog,
                "abandonne" | "abandoned" | "dropped" | "abgebrochen" | "abandonado" | "abbandonato" => PlayStatus::Abandoned,
                "aucun" | "none" | "kein" | "keiner" | "ninguno" | "nessuno" | "nenhum" => PlayStatus::None,
                _ => return None,
            })
        }
        "temps" | "playtime" | "time" | "spielzeit" | "tiempo" | "tempo" => {
            let (cmp, rest) = parse_cmp(value);
            Filter::Playtime(cmp, parse_duration(rest)?)
        }
        "succes" | "achievements" | "erfolge" | "logros" | "obiettivi" | "conquistas" => {
            let v = util::search_key(value).replace(' ', "");
            Filter::Achievements(match v.as_str() {
                "complet" | "complets" | "100" | "complete" | "vollstaendig" | "vollstandig" | "komplett" | "completo" | "completos" | "completi" => AchFilter::Complete,
                "manquants" | "manquant" | "incomplet" | "missing" | "fehlend" | "fehlende" | "faltan" | "pendientes" | "mancanti" | "emfalta" | "faltam" => AchFilter::Missing,
                "aucun" | "none" | "keine" | "ninguno" | "nessuno" | "nenhum" => AchFilter::None,
                "oui" | "any" | "yes" | "ja" | "si" | "sim" => AchFilter::Any,
                _ => {
                    let (cmp, rest) = parse_cmp(value);
                    let n: u64 = rest.trim_end_matches('%').trim().parse().ok()?;
                    AchFilter::Percent(if cmp == Cmp::Eq { Cmp::Ge } else { cmp }, n.min(100))
                }
            })
        }
        _ => return None,
    })
}

fn contains_key(hay: Option<&str>, needle: &str) -> bool {
    hay.is_some_and(|h| util::search_key(h).contains(needle))
}

pub(crate) fn filter_matches(f: &Filter, g: &Game, ctx: &QueryContext) -> bool {
    match f {
        Filter::Developer(v) => contains_key(g.developer.as_deref(), v),
        Filter::Publisher(v) => contains_key(g.publisher.as_deref(), v),
        Filter::Genre(v) => g.genres.iter().any(|x| util::search_key(x).contains(v.as_str())),
        Filter::Tag(v) => g.tags.iter().any(|x| util::search_key(x).contains(v.as_str())),
        Filter::Platform(v) => {
            let source_hit = util::search_key(g.source.as_str()).contains(v.as_str())
                || util::search_key(&g.source.label()).contains(v.as_str())
                || (v == "standalone" && g.source == GameSource::Manual);
            source_hit
                || g.platform.as_deref().is_some_and(|p| {
                    util::search_key(p) == *v
                        || ctx.platform_names.get(p).is_some_and(|name| util::search_key(name).contains(v.as_str()))
                })
        }
        Filter::Installed(b) => g.installed == *b,
        Filter::Favorite(b) => g.favorite == *b,
        Filter::NeedsUpdate(b) => g.update.state.pending() == *b,
        Filter::Status(s) => g.status == *s,
        Filter::Playtime(cmp, secs) => cmp.test(g.effective_playtime(), *secs),
        Filter::Achievements(a) => {
            let p = ctx.progress.get(&g.id).filter(|p| p.total > 0);
            match a {
                AchFilter::Any => p.is_some(),
                AchFilter::None => p.is_none(),
                AchFilter::Complete => p.is_some_and(|p| p.complete()),
                AchFilter::Missing => p.is_some_and(|p| !p.complete()),
                AchFilter::Percent(cmp, n) => p.is_some_and(|p| cmp.test((p.percent() * 100.0) as u64, *n * 100)),
            }
        }
        Filter::Not(inner) => !filter_matches(inner, g, ctx),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(id: i64, name: &str) -> Game {
        Game::blank(id, name, GameSource::Steam)
    }

    #[test]
    fn parses_fields_quotes_and_negation() {
        let q = SearchQuery::parse("witcher DÉV:\"CD Projekt\" -installé:oui temps:>10h succès:>=80 foo:bar");
        assert_eq!(q.terms, ["witcher", "foo", "bar"], "champ inconnu = texte libre");
        assert_eq!(q.filters[0], Filter::Developer("cd projekt".into()));
        assert_eq!(q.filters[1], Filter::Not(Box::new(Filter::Installed(true))));
        assert_eq!(q.filters[2], Filter::Playtime(Cmp::Gt, 36_000));
        assert_eq!(q.filters[3], Filter::Achievements(AchFilter::Percent(Cmp::Ge, 80)));
        assert!(SearchQuery::parse("statut:bidule").errors.len() == 1);
        assert_eq!(parse_duration("1h30"), Some(5400));
        assert_eq!(parse_duration("45m"), Some(2700));
        assert_eq!(parse_duration("2,5"), Some(9000));
    }

    #[test]
    fn fuzzy_and_acronyms() {
        let ctx = QueryContext::default();
        let w = game(1, "The Witcher 3: Wild Hunt");
        let gta = game(2, "Grand Theft Auto V");
        let m = |s: &str, g: &Game| SearchQuery::parse(s).matches(g, &ctx);
        assert_eq!(m("witcher", &w), Some(MatchTier::Direct));
        assert_eq!(m("wticher", &w), Some(MatchTier::Fuzzy), "transposition");
        assert_eq!(m("witchr", &w), Some(MatchTier::Fuzzy), "lettre manquante");
        assert_eq!(m("witc", &w), Some(MatchTier::Direct), "préfixe");
        assert_eq!(m("gta", &gta), Some(MatchTier::Direct), "sigle");
        assert_eq!(m("zelda", &w), None);
        assert_eq!(m("hu", &gta), None, "pas de flou sous 3 lettres");
        assert_eq!(distance("abc", "acb"), 1);
    }

    #[test]
    fn filters_use_context() {
        let mut g = game(7, "Elden Ring");
        g.developer = Some("FromSoftware".into());
        g.genres = vec!["Action RPG".into()];
        g.tags = vec!["multijoueur".into()];
        g.playtime_secs = 20 * 3600;
        g.status = PlayStatus::Playing;
        let mut ctx = QueryContext::default();
        ctx.progress.insert(7, Progress { unlocked: 34, total: 42 });
        let ok = |s: &str| SearchQuery::parse(s).matches(&g, &ctx).is_some();
        assert!(ok("dev:fromsoft genre:rpg tag:multi"));
        assert!(ok("statut:\"en cours\" temps:>=20h succès:manquants succès:>80"));
        assert!(!ok("succès:complet"));
        assert!(!ok("temps:<1h"));
        assert!(ok("source:steam -favori:oui"));
        g.playtime_manual_secs = Some(600);
        assert!(SearchQuery::parse("temps:<1h").matches(&g, &ctx).is_some(), "la correction manuelle prime");
    }
}

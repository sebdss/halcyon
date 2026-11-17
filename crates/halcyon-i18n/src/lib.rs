//! Internationalisation de Halcyon.
//!
//! - Les textes sont écrits en **français** dans le code (langue de référence) et servent
//!   de clés : `tr!("Jouer")`, `@tr("Jouer")` côté Slint.
//! - Les traductions sont des fichiers gettext `.po` dans `locales/<langue>/<domaine>.po`,
//!   assemblés et intégrés au binaire à la compilation (aucun accès réseau, aucun fichier
//!   externe à l'exécution) ; voir `docs/i18n/README.md`.
//! - Recherche : langue choisie → anglais (langue de repli) → texte source.
//! - Pluriels : règle `Plural-Forms` de chaque catalogue.
//! - Formats : séparateurs de nombres lus dans l'en-tête du catalogue
//!   (`X-Halcyon-Thousands-Separator`, `X-Halcyon-Decimal-Separator`) ; dates, unités et
//!   tournures sont des textes traduisibles comme les autres.
//!
//! Le module est sûr entre fils d'exécution : l'interface et les tâches de fond peuvent
//! traduire en même temps.

pub mod build;
pub mod plural;
pub mod po;

use std::collections::HashMap;
use std::sync::RwLock;

/// Langue de référence : celle des textes écrits dans le code.
pub const REFERENCE: &str = "fr-FR";
/// Langue de repli quand la langue du système n'est pas disponible ou qu'une traduction manque.
pub const FALLBACK: &str = "en-US";

/// Une langue disponible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Language {
    /// Code BCP 47 (« de-DE »), nom du dossier dans `locales/`.
    pub code: String,
    /// Nom dans la langue elle-même (« Deutsch »).
    pub native_name: String,
}

struct Catalog {
    native_name: String,
    rule: plural::Expr,
    nplurals: usize,
    thousands: String,
    decimal: String,
    entries: HashMap<po::Key, Vec<String>>,
}

impl Catalog {
    fn from_po(code: &str, text: &str) -> Result<Catalog, String> {
        let file = po::PoFile::parse(text).map_err(|e| format!("{code} : {e}"))?;
        let (nplurals, rule) = match file.plural_forms() {
            Some((n, expr)) => (n, plural::parse(&expr).map_err(|e| format!("{code} : Plural-Forms : {e}"))?),
            None => (2, plural::parse("n != 1").unwrap()),
        };
        let h = |k: &str| file.headers.get(k).cloned();
        Ok(Catalog {
            native_name: h("X-Halcyon-Native-Name").unwrap_or_else(|| code.to_string()),
            thousands: h("X-Halcyon-Thousands-Separator").map(|s| unescape_sep(&s)).unwrap_or_else(|| ",".into()),
            decimal: h("X-Halcyon-Decimal-Separator").map(|s| unescape_sep(&s)).unwrap_or_else(|| ".".into()),
            rule,
            nplurals,
            entries: po::index(&file),
        })
    }
}

/// Les séparateurs peuvent être écrits `U+00A0` (espace insécable ; les polices embarquées n’ont pas l’espace fine U+202F) dans l'en-tête.
fn unescape_sep(s: &str) -> String {
    match s.strip_prefix("U+").and_then(|h| u32::from_str_radix(h, 16).ok()).and_then(char::from_u32) {
        Some(c) => c.to_string(),
        None => s.to_string(),
    }
}

struct State {
    catalogs: Vec<(String, Catalog)>,
    current: usize,
    fallback: Option<usize>,
}

static STATE: RwLock<Option<State>> = RwLock::new(None);

/// Enregistre les catalogues intégrés au binaire : `(code, contenu .po)`.
/// La langue active devient la langue de référence tant que [`set_language`] n'est pas appelé.
pub fn register(catalogs: &[(&str, &str)]) -> Result<(), String> {
    let mut list = Vec::new();
    for (code, text) in catalogs {
        list.push((code.to_string(), Catalog::from_po(code, text)?));
    }
    list.sort_by(|a, b| a.0.cmp(&b.0));
    let fallback = list.iter().position(|(c, _)| c == FALLBACK);
    let current = list.iter().position(|(c, _)| c == REFERENCE).or(fallback).unwrap_or(0);
    *STATE.write().unwrap_or_else(|e| e.into_inner()) = Some(State { catalogs: list, current, fallback });
    Ok(())
}

/// Langues disponibles, dans l'ordre alphabétique des codes.
pub fn languages() -> Vec<Language> {
    let st = STATE.read().unwrap_or_else(|e| e.into_inner());
    st.as_ref()
        .map(|s| s.catalogs.iter().map(|(c, cat)| Language { code: c.clone(), native_name: cat.native_name.clone() }).collect())
        .unwrap_or_default()
}

/// Code de la langue active.
pub fn language() -> String {
    let st = STATE.read().unwrap_or_else(|e| e.into_inner());
    st.as_ref().and_then(|s| s.catalogs.get(s.current)).map(|(c, _)| c.clone()).unwrap_or_else(|| REFERENCE.into())
}

/// Active une langue (doit être disponible). Renvoie `false` sinon, sans rien changer.
pub fn set_language(code: &str) -> bool {
    let mut st = STATE.write().unwrap_or_else(|e| e.into_inner());
    let Some(s) = st.as_mut() else { return false };
    match s.catalogs.iter().position(|(c, _)| c == code) {
        Some(i) => {
            s.current = i;
            true
        }
        None => false,
    }
}

/// Choisit la langue à utiliser parmi `available` :
/// préférence enregistrée si disponible, sinon langues du système dans l'ordre (code exact,
/// puis même langue d'une autre région : « de-AT » → « de-DE »), sinon l'anglais.
pub fn resolve<'a>(preferred: Option<&str>, system: impl IntoIterator<Item = String>, available: &'a [String]) -> String {
    if let Some(p) = preferred.filter(|p| !p.is_empty()) {
        if let Some(a) = available.iter().find(|a| a.eq_ignore_ascii_case(p)) {
            return a.clone();
        }
    }
    let base = |s: &str| s.split(['-', '_', '.', '@']).next().unwrap_or("").to_ascii_lowercase();
    for loc in system {
        let norm = loc.split(['.', '@']).next().unwrap_or("").replace('_', "-");
        if let Some(a) = available.iter().find(|a| a.eq_ignore_ascii_case(&norm)) {
            return a.clone();
        }
        let b = base(&norm);
        if b.is_empty() || b == "c" || b == "posix" {
            continue;
        }
        if let Some(a) = available.iter().find(|a| base(a) == b) {
            return a.clone();
        }
    }
    if available.iter().any(|a| a == FALLBACK) {
        FALLBACK.into()
    } else {
        available.first().cloned().unwrap_or_else(|| REFERENCE.into())
    }
}

/// Langues préférées du système d'exploitation, dans l'ordre.
pub fn system_locales() -> Vec<String> {
    sys_locale::get_locales().collect()
}

/// La langue du système fait-elle partie des langues disponibles (même région ou non) ?
/// Sinon, [`resolve`] se replie sur l'anglais.
pub fn system_language_supported(system: &[String], available: &[String]) -> bool {
    let base = |s: &str| s.split(['-', '_', '.', '@']).next().unwrap_or("").to_ascii_lowercase();
    system.iter().any(|loc| {
        let b = base(loc);
        !b.is_empty() && b != "c" && b != "posix" && available.iter().any(|a| base(a) == b)
    })
}

/// Langue à utiliser au démarrage : préférence enregistrée, sinon détection du système.
pub fn startup_language(preferred: Option<&str>) -> String {
    let available: Vec<String> = languages().into_iter().map(|l| l.code).collect();
    resolve(preferred, system_locales(), &available)
}

fn lookup(context: Option<&str>, id: &str, plural: Option<(&str, u64)>) -> String {
    let st = STATE.read().unwrap_or_else(|e| e.into_inner());
    // texte source (français) : le singulier couvre 0 et 1
    let source = || match plural {
        Some((other, n)) if n > 1 => other.to_string(),
        _ => id.to_string(),
    };
    let Some(s) = st.as_ref() else { return source() };
    let key = (context.map(str::to_string), id.to_string());
    // en français, un texte absent du catalogue est le texte source lui-même
    let fallback = if s.catalogs[s.current].0 == REFERENCE { None } else { s.fallback };
    for idx in [Some(s.current), fallback].into_iter().flatten() {
        let cat = &s.catalogs[idx].1;
        if let Some(strs) = cat.entries.get(&key) {
            let form = match plural {
                Some((_, n)) => (cat.rule.eval(n) as usize).min(cat.nplurals.saturating_sub(1)),
                None => 0,
            };
            if let Some(t) = strs.get(form).or(strs.first()) {
                return t.clone();
            }
        }
    }
    source()
}

/// Traduit un texte simple.
pub fn t(id: &str) -> String {
    lookup(None, id, None)
}

/// Traduit un texte avec contexte (homonymes : « mai » le mois…).
pub fn tc(context: &str, id: &str) -> String {
    lookup(Some(context), id, None)
}

/// Traduit un texte au pluriel selon `n`.
pub fn tn(id: &str, id_plural: &str, n: u64) -> String {
    lookup(None, id, Some((id_plural, n)))
}

/// Traduit un texte au pluriel selon `n`, avec contexte.
pub fn tcn(context: &str, id: &str, id_plural: &str, n: u64) -> String {
    lookup(Some(context), id, Some((id_plural, n)))
}

/// Remplace les variables `{nom}` d'un texte traduit. Une variable inconnue reste telle quelle.
pub fn fill(template: &str, args: &[(&str, String)]) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('}') {
            Some(end) => {
                let name = &after[..end];
                match args.iter().find(|(k, _)| *k == name) {
                    Some((_, v)) => out.push_str(v),
                    None => {
                        out.push('{');
                        out.push_str(name);
                        out.push('}');
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

fn separators() -> (String, String) {
    let st = STATE.read().unwrap_or_else(|e| e.into_inner());
    st.as_ref()
        .and_then(|s| s.catalogs.get(s.current))
        .map(|(_, c)| (c.thousands.clone(), c.decimal.clone()))
        .unwrap_or_else(|| ("\u{A0}".into(), ",".into()))
}

/// Met en minuscule la première lettre d'un texte repris au milieu d'une phrase
/// (« Jamais lancé » → « jamais lancé »). En allemand, les noms gardent leur majuscule :
/// le texte est laissé tel quel.
pub fn lower_first(s: &str) -> String {
    if language().starts_with("de") {
        return s.to_string();
    }
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_lowercase().chain(c).collect(),
        None => String::new(),
    }
}

/// Entier avec séparateur de milliers de la langue active : « 1 284 », « 1,284 », « 1.284 ».
pub fn number(n: u64) -> String {
    let (sep, _) = separators();
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push_str(&sep);
        }
        out.push(c);
    }
    out
}

/// Nombre décimal avec `digits` décimales et le séparateur de la langue active.
pub fn decimal(v: f64, digits: usize) -> String {
    let (_, dec) = separators();
    let s = format!("{v:.digits$}");
    if dec == "." {
        s
    } else {
        s.replace('.', &dec)
    }
}

const DEFER: char = '\u{1}';
const ARG: char = '\u{1f}';
const KV: char = '\u{1e}';

/// Texte à traduire **à l'affichage** plutôt qu'à sa création : pour ce qui est enregistré
/// (journal d'activité) et relu plus tard, éventuellement dans une autre langue.
/// Produit une forme compacte que [`render`] traduit ; voir [`tr_later!`].
pub fn defer(id: &str, args: &[(&str, String)]) -> String {
    let mut s = String::with_capacity(id.len() + 16);
    s.push(DEFER);
    s.push_str(id);
    for (k, v) in args {
        s.push(ARG);
        s.push_str(k);
        s.push(KV);
        s.push_str(&v.replace([DEFER, ARG, KV], " "));
    }
    s
}

/// Traduit un texte produit par [`defer`] dans la langue active ; tout autre texte est
/// renvoyé tel quel (entrées enregistrées avant l'internationalisation).
pub fn render(stored: &str) -> String {
    let Some(rest) = stored.strip_prefix(DEFER) else { return stored.to_string() };
    let mut parts = rest.split(ARG);
    let id = parts.next().unwrap_or("");
    let mut args: Vec<(&str, String)> = parts.filter_map(|p| p.split_once(KV)).map(|(k, v)| (k, v.to_string())).collect();
    let plural = args.iter().find(|(k, _)| *k == "#plural").map(|(_, v)| v.clone());
    let n = args.iter().find(|(k, _)| *k == "#n").and_then(|(_, v)| v.parse::<u64>().ok());
    args.retain(|(k, _)| !k.starts_with('#'));
    match (plural, n) {
        (Some(p), Some(n)) => {
            args.push(("n", number(n)));
            fill(&tn(id, &p, n), &args)
        }
        _ => fill(&t(id), &args),
    }
}

/// Traduit un texte : `tr!("Jouer")`, `tr!("Bienvenue, {nom}", nom = nom)`.
#[macro_export]
macro_rules! tr {
    ($msg:literal) => {
        $crate::t($msg)
    };
    ($msg:literal, $($k:ident = $v:expr),+ $(,)?) => {
        $crate::fill(&$crate::t($msg), &[$((stringify!($k), ($v).to_string())),+])
    };
}

/// Traduit un texte avec contexte : `trc!("month", "mai")`.
#[macro_export]
macro_rules! trc {
    ($ctx:literal, $msg:literal) => {
        $crate::tc($ctx, $msg)
    };
    ($ctx:literal, $msg:literal, $($k:ident = $v:expr),+ $(,)?) => {
        $crate::fill(&$crate::tc($ctx, $msg), &[$((stringify!($k), ($v).to_string())),+])
    };
}

/// Traduit un texte au pluriel ; `{n}` est remplacé par `n` formaté selon la langue :
/// `trn!(count, "{n} jeu", "{n} jeux")`, `trn!(c, "{n} fichier dans {dir}", "{n} fichiers dans {dir}", dir = d)`.
#[macro_export]
macro_rules! trn {
    ($n:expr, $one:literal, $many:literal $(, $k:ident = $v:expr)* $(,)?) => {{
        let n = ($n) as u64;
        $crate::fill(&$crate::tn($one, $many, n), &[("n", $crate::number(n)) $(, (stringify!($k), ($v).to_string()))*])
    }};
}

/// Texte enregistré et traduit à l'affichage (voir [`defer`], [`render`]) :
/// `tr_later!("{name} installé", name = game)`.
#[macro_export]
macro_rules! tr_later {
    ($msg:literal) => {
        $crate::defer($msg, &[])
    };
    ($msg:literal, $($k:ident = $v:expr),+ $(,)?) => {
        $crate::defer($msg, &[$((stringify!($k), ($v).to_string())),+])
    };
}

/// Pluriel enregistré et traduit à l'affichage : `trn_later!(n, "{n} jeu ajouté", "{n} jeux ajoutés")`.
#[macro_export]
macro_rules! trn_later {
    ($n:expr, $one:literal, $many:literal $(, $k:ident = $v:expr)* $(,)?) => {{
        let n = ($n) as u64;
        $crate::defer($one, &[("#plural", $many.to_string()), ("#n", n.to_string()) $(, (stringify!($k), ($v).to_string()))*])
    }};
}

/// Marque un texte à traduire plus tard (tableaux statiques) ; renvoie le texte source.
/// À l'affichage : [`t`] sur la valeur.
#[macro_export]
macro_rules! tr_noop {
    ($msg:literal) => {
        $msg
    };
}

/// Comme [`tr_noop!`], avec un contexte (à l'affichage : [`tc`]).
#[macro_export]
macro_rules! trc_noop {
    ($ctx:literal, $msg:literal) => {
        $msg
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    pub(crate) static LOCK: Mutex<()> = Mutex::new(());

    const FR: &str = "msgid \"\"\nmsgstr \"\"\n\"Plural-Forms: nplurals=2; plural=(n > 1);\\n\"\n\"X-Halcyon-Native-Name: Français\\n\"\n\"X-Halcyon-Thousands-Separator: U+00A0\\n\"\n\"X-Halcyon-Decimal-Separator: ,\\n\"\n";
    const EN: &str = "msgid \"\"\nmsgstr \"\"\n\"Plural-Forms: nplurals=2; plural=(n != 1);\\n\"\n\"X-Halcyon-Native-Name: English\\n\"\n\"X-Halcyon-Thousands-Separator: ,\\n\"\n\"X-Halcyon-Decimal-Separator: .\\n\"\n\nmsgid \"Jouer\"\nmsgstr \"Play\"\n\nmsgid \"Bonjour {nom}\"\nmsgstr \"Hello {nom}\"\n\nmsgid \"{n} jeu\"\nmsgid_plural \"{n} jeux\"\nmsgstr[0] \"{n} game\"\nmsgstr[1] \"{n} games\"\n\nmsgid \"Quitter\"\nmsgstr \"Quit\"\n\nmsgctxt \"month\"\nmsgid \"mai\"\nmsgstr \"May\"\n";
    const DE: &str = "msgid \"\"\nmsgstr \"\"\n\"Plural-Forms: nplurals=2; plural=(n != 1);\\n\"\n\"X-Halcyon-Native-Name: Deutsch\\n\"\n\"X-Halcyon-Thousands-Separator: .\\n\"\n\"X-Halcyon-Decimal-Separator: ,\\n\"\n\nmsgid \"Jouer\"\nmsgstr \"Spielen\"\n\nmsgid \"{n} jeu\"\nmsgid_plural \"{n} jeux\"\nmsgstr[0] \"{n} Spiel\"\nmsgstr[1] \"{n} Spiele\"\n\nmsgid \"Quitter\"\nmsgstr \"\"\n";

    fn setup() -> std::sync::MutexGuard<'static, ()> {
        let g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        register(&[("fr-FR", FR), ("en-US", EN), ("de-DE", DE)]).unwrap();
        g
    }

    #[test]
    fn reference_language_returns_source_text() {
        let _g = setup();
        assert_eq!(language(), "fr-FR");
        assert_eq!(tr!("Jouer"), "Jouer");
        assert_eq!(trn!(0, "{n} jeu", "{n} jeux"), "0 jeu");
        assert_eq!(trn!(1, "{n} jeu", "{n} jeux"), "1 jeu");
        assert_eq!(trn!(12_500, "{n} jeu", "{n} jeux"), "12\u{A0}500 jeux");
    }

    #[test]
    fn switching_language_and_plurals() {
        let _g = setup();
        assert!(set_language("en-US"));
        assert_eq!(tr!("Jouer"), "Play");
        assert_eq!(tr!("Bonjour {nom}", nom = "Sebi"), "Hello Sebi");
        assert_eq!(trn!(0, "{n} jeu", "{n} jeux"), "0 games");
        assert_eq!(trn!(1, "{n} jeu", "{n} jeux"), "1 game");
        assert_eq!(trn!(1284, "{n} jeu", "{n} jeux"), "1,284 games");
        assert_eq!(trc!("month", "mai"), "May");
        assert_eq!(decimal(48.25, 1), "48.2");
        assert!(set_language("de-DE"));
        assert_eq!(trn!(2, "{n} jeu", "{n} jeux"), "2 Spiele");
        assert_eq!(number(12_345), "12.345");
        assert_eq!(decimal(48.25, 1), "48,2");
    }

    #[test]
    fn missing_translation_falls_back_to_english() {
        let _g = setup();
        set_language("de-DE");
        assert_eq!(tr!("Quitter"), "Quit", "traduction vide → anglais");
        assert_eq!(tr!("Bonjour {nom}", nom = "Sebi"), "Hello Sebi", "absente → anglais");
        assert_eq!(tr!("Texte inconnu"), "Texte inconnu", "absente partout → source");
        assert!(!set_language("ja-JP"), "langue indisponible refusée");
        assert_eq!(language(), "de-DE");
    }

    #[test]
    fn resolves_preference_system_and_fallback() {
        let av: Vec<String> = ["de-DE", "en-US", "es-ES", "fr-FR", "it-IT", "pt-PT"].map(String::from).to_vec();
        let sys = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(resolve(Some("it-IT"), sys(&["fr-FR"]), &av), "it-IT");
        assert_eq!(resolve(Some("xx-XX"), sys(&["fr-FR"]), &av), "fr-FR", "préférence inconnue ignorée");
        assert_eq!(resolve(None, sys(&["de-AT"]), &av), "de-DE");
        assert_eq!(resolve(None, sys(&["pt_BR.UTF-8"]), &av), "pt-PT");
        assert_eq!(resolve(None, sys(&["fr_FR.UTF-8"]), &av), "fr-FR");
        assert_eq!(resolve(None, sys(&["ja-JP", "es-MX"]), &av), "es-ES", "deuxième langue du système");
        assert_eq!(resolve(None, sys(&["ja-JP"]), &av), "en-US", "langue indisponible → anglais");
        assert_eq!(resolve(None, sys(&["C"]), &av), "en-US");
        assert_eq!(resolve(None, Vec::new(), &av), "en-US");
    }

    #[test]
    fn fill_keeps_unknown_and_unbalanced() {
        assert_eq!(fill("{a} et {b}", &[("a", "1".into())]), "1 et {b}");
        assert_eq!(fill("accolade { seule", &[]), "accolade { seule");
    }

    #[test]
    fn deferred_text_is_translated_when_displayed() {
        let _g = setup();
        set_language("fr-FR");
        let stored = tr_later!("Bonjour {nom}", nom = "Sebi");
        assert_eq!(render(&stored), "Bonjour Sebi");
        set_language("en-US");
        assert_eq!(render(&stored), "Hello Sebi", "même entrée, autre langue");
        assert_eq!(render("Ancienne entrée"), "Ancienne entrée");
        let stored = trn_later!(1284, "{n} jeu", "{n} jeux");
        assert_eq!(render(&stored), "1,284 games");
        set_language("fr-FR");
        assert_eq!(render(&stored), "1\u{A0}284 jeux");
    }

    #[test]
    fn languages_listed_with_native_names() {
        let _g = setup();
        let l = languages();
        assert_eq!(l.iter().map(|x| x.native_name.as_str()).collect::<Vec<_>>(), ["Deutsch", "English", "Français"]);
    }
}

//! Lecture des fichiers gettext `.po` (sous-ensemble utilisé par Halcyon et par Slint).
//!
//! Prend en charge : en-tête, `msgctxt`, `msgid`, `msgid_plural`, `msgstr`, `msgstr[n]`,
//! chaînes sur plusieurs lignes, échappements `\n \t \" \\`, commentaires, entrées
//! obsolètes (`#~`, ignorées). Une entrée dont la traduction est vide est ignorée : la
//! recherche passe alors à la langue de repli.

use std::collections::HashMap;

/// Une entrée traduite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub context: Option<String>,
    pub id: String,
    pub id_plural: Option<String>,
    /// Une forme (texte simple) ou une forme par cas de pluriel.
    pub strs: Vec<String>,
    /// Drapeau `#, fuzzy` : traduction à revoir (non utilisée).
    pub fuzzy: bool,
}

/// Contenu d'un fichier `.po`.
#[derive(Debug, Default, Clone)]
pub struct PoFile {
    pub headers: HashMap<String, String>,
    pub entries: Vec<Entry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ligne {} : {}", self.line, self.message)
    }
}

impl std::error::Error for ParseError {}

#[derive(Default)]
struct Pending {
    context: Option<String>,
    id: Option<String>,
    id_plural: Option<String>,
    strs: Vec<(usize, String)>,
    fuzzy: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum Field {
    None,
    Context,
    Id,
    IdPlural,
    Str(usize),
}

fn unescape(s: &str, line: usize) -> Result<String, ParseError> {
    let s = s.trim();
    if s.len() < 2 || !s.starts_with('"') || !s.ends_with('"') {
        return Err(ParseError { line, message: format!("chaîne attendue entre guillemets : {s}") });
    }
    let inner = &s[1..s.len() - 1];
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some(o) => return Err(ParseError { line, message: format!("échappement inconnu \\{o}") }),
            None => return Err(ParseError { line, message: "barre oblique finale".into() }),
        }
    }
    Ok(out)
}

/// Échappe une chaîne pour l'écrire dans un `.po`.
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

impl PoFile {
    pub fn parse(text: &str) -> Result<PoFile, ParseError> {
        let mut file = PoFile::default();
        let mut cur = Pending::default();
        let mut field = Field::None;
        let mut header_seen = false;

        let mut flush = |cur: &mut Pending, file: &mut PoFile, line: usize| -> Result<(), ParseError> {
            let p = std::mem::take(cur);
            let Some(id) = p.id else {
                if p.context.is_some() || !p.strs.is_empty() {
                    return Err(ParseError { line, message: "entrée sans msgid".into() });
                }
                return Ok(());
            };
            let mut strs = p.strs;
            strs.sort_by_key(|(i, _)| *i);
            let strs: Vec<String> = strs.into_iter().map(|(_, s)| s).collect();
            if id.is_empty() && p.context.is_none() {
                if !header_seen {
                    for l in strs.first().map(String::as_str).unwrap_or("").lines() {
                        if let Some((k, v)) = l.split_once(':') {
                            file.headers.insert(k.trim().to_string(), v.trim().to_string());
                        }
                    }
                    header_seen = true;
                }
                return Ok(());
            }
            file.entries.push(Entry { context: p.context, id, id_plural: p.id_plural, strs, fuzzy: p.fuzzy });
            Ok(())
        };

        let lines: Vec<&str> = text.lines().collect();
        for (i, raw) in lines.iter().enumerate() {
            let n = i + 1;
            let line = raw.trim();
            if line.is_empty() {
                flush(&mut cur, &mut file, n)?;
                field = Field::None;
                continue;
            }
            if let Some(rest) = line.strip_prefix('#') {
                if rest.starts_with('~') {
                    continue;
                }
                if field != Field::None && field != Field::Context {
                    // un commentaire commence une nouvelle entrée
                    flush(&mut cur, &mut file, n)?;
                    field = Field::None;
                }
                if let Some(flags) = rest.strip_prefix(',') {
                    if flags.split(',').any(|f| f.trim() == "fuzzy") {
                        cur.fuzzy = true;
                    }
                }
                continue;
            }
            if line.starts_with('"') {
                let s = unescape(line, n)?;
                match field {
                    Field::Context => cur.context.get_or_insert_with(String::new).push_str(&s),
                    Field::Id => cur.id.get_or_insert_with(String::new).push_str(&s),
                    Field::IdPlural => cur.id_plural.get_or_insert_with(String::new).push_str(&s),
                    Field::Str(k) => {
                        if let Some(e) = cur.strs.iter_mut().find(|(i, _)| *i == k) {
                            e.1.push_str(&s);
                        }
                    }
                    Field::None => return Err(ParseError { line: n, message: "chaîne hors d'une entrée".into() }),
                }
                continue;
            }
            let (kw, value) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
            if kw == "msgctxt" || (kw == "msgid" && matches!(field, Field::Str(_))) {
                flush(&mut cur, &mut file, n)?;
            }
            match kw {
                "msgctxt" => {
                    cur.context = Some(unescape(value, n)?);
                    field = Field::Context;
                }
                "msgid" => {
                    cur.id = Some(unescape(value, n)?);
                    field = Field::Id;
                }
                "msgid_plural" => {
                    cur.id_plural = Some(unescape(value, n)?);
                    field = Field::IdPlural;
                }
                "msgstr" => {
                    cur.strs.push((0, unescape(value, n)?));
                    field = Field::Str(0);
                }
                k if k.starts_with("msgstr[") && k.ends_with(']') => {
                    let idx: usize = k[7..k.len() - 1]
                        .parse()
                        .map_err(|_| ParseError { line: n, message: format!("index de pluriel invalide : {k}") })?;
                    cur.strs.push((idx, unescape(value, n)?));
                    field = Field::Str(idx);
                }
                other => return Err(ParseError { line: n, message: format!("mot-clé inconnu : {other}") }),
            }
        }
        flush(&mut cur, &mut file, lines.len() + 1)?;
        Ok(file)
    }

    /// Expression de pluriel de l'en-tête (`Plural-Forms: nplurals=2; plural=(n > 1);`).
    pub fn plural_forms(&self) -> Option<(usize, String)> {
        let pf = self.headers.get("Plural-Forms")?;
        let mut n = None;
        let mut expr = None;
        for part in pf.split(';') {
            if let Some((k, v)) = part.split_once('=') {
                match k.trim() {
                    "nplurals" => n = v.trim().parse().ok(),
                    "plural" => expr = Some(part[part.find('=')? + 1..].trim().to_string()),
                    _ => {}
                }
            }
        }
        Some((n?, expr?))
    }
}

/// Clé de recherche : contexte facultatif et texte source.
pub(crate) type Key = (Option<String>, String);

pub(crate) fn index(file: &PoFile) -> HashMap<Key, Vec<String>> {
    file.entries
        .iter()
        .filter(|e| !e.fuzzy && !e.strs.is_empty() && e.strs.iter().all(|s| !s.is_empty()))
        .map(|e| ((e.context.clone(), e.id.clone()), e.strs.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
# en-tête
msgid ""
msgstr ""
"Language: de-DE\n"
"Plural-Forms: nplurals=2; plural=(n != 1);\n"

#: ui/app.slint:10
msgid "Jouer"
msgstr "Spielen"

msgctxt "month"
msgid "mai"
msgstr "Mai"

msgid "{n} jeu"
msgid_plural "{n} jeux"
msgstr[0] "{n} Spiel"
msgstr[1] "{n} Spiele"

msgid "Sur "
"deux lignes"
msgstr "Auf "
"zwei Zeilen"

#, fuzzy
msgid "Brouillon"
msgstr "Entwurf"

msgid "Vide"
msgstr ""

#~ msgid "Ancien"
#~ msgstr "Alt"
"#;

    #[test]
    fn parses_entries_headers_and_plurals() {
        let f = PoFile::parse(SAMPLE).unwrap();
        assert_eq!(f.headers["Language"], "de-DE");
        assert_eq!(f.plural_forms(), Some((2, "(n != 1)".into())));
        assert_eq!(f.entries.len(), 6);
        let idx = index(&f);
        assert_eq!(idx[&(None, "Jouer".into())], vec!["Spielen"]);
        assert_eq!(idx[&(Some("month".into()), "mai".into())], vec!["Mai"]);
        assert_eq!(idx[&(None, "{n} jeu".into())], vec!["{n} Spiel", "{n} Spiele"]);
        assert_eq!(idx[&(None, "Sur deux lignes".into())], vec!["Auf zwei Zeilen"]);
        assert!(!idx.contains_key(&(None, "Brouillon".into())), "fuzzy ignorée");
        assert!(!idx.contains_key(&(None, "Vide".into())), "vide ignorée");
    }

    #[test]
    fn escape_round_trip() {
        let s = "Ligne \"citée\"\n\\fin";
        let po = format!("msgid {}\nmsgstr {}\n", escape(s), escape(s));
        let f = PoFile::parse(&po).unwrap();
        assert_eq!(f.entries[0].id, s);
    }

    #[test]
    fn reports_errors_with_line() {
        let e = PoFile::parse("msgid \"a\"\nmsgstr \"b\\q\"\n").unwrap_err();
        assert_eq!(e.line, 2);
    }
}

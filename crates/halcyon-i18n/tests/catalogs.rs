//! Vérifie les catalogues réels de `locales/` : fichiers valides, aucune clé manquante,
//! aucune traduction vide ou « fuzzy », variables identiques à la source, nombre de formes
//! de pluriel conforme à l'en-tête, catalogues chargeables et utilisables à l'exécution.
//!
//! Lancer : `cargo test -p halcyon-i18n --test catalogs`.

use halcyon_i18n::po::{Entry, PoFile};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const LANGUAGES: [&str; 6] = ["de-DE", "en-US", "es-ES", "fr-FR", "it-IT", "pt-PT"];

fn locales() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../locales")
}

fn domains() -> Vec<String> {
    let mut d: Vec<String> = std::fs::read_dir(locales().join("templates"))
        .expect("locales/templates")
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().to_str()?.strip_suffix(".pot").map(String::from))
        .collect();
    d.sort();
    assert!(!d.is_empty(), "aucun modèle .pot");
    d
}

fn read(path: &Path) -> PoFile {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{} : {e}", path.display()));
    PoFile::parse(&text).unwrap_or_else(|e| panic!("{} : {e}", path.display()))
}

type Key = (Option<String>, String);

fn key(e: &Entry) -> Key {
    (e.context.clone(), e.id.clone())
}

fn show(k: &Key) -> String {
    match &k.0 {
        Some(c) => format!("[{c}] {}", k.1),
        None => k.1.clone(),
    }
}

/// Variables d'un texte : `{nom}` (Rust), `{}` / `{0}` / `{n}` (Slint). `{{` et `}}` sont
/// des accolades littérales. Les `{}` anonymes sont numérotés dans l'ordre, comme Slint.
fn placeholders(s: &str) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    let mut anon = 0;
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'{' if b.get(i + 1) == Some(&b'{') => i += 2,
            b'}' if b.get(i + 1) == Some(&b'}') => i += 2,
            b'{' => {
                let end = s[i..].find('}').map(|j| i + j).unwrap_or(b.len());
                let mut name = s[i + 1..end.min(b.len())].to_string();
                if name.is_empty() {
                    name = anon.to_string();
                    anon += 1;
                }
                *out.entry(name).or_insert(0) += 1;
                i = end + 1;
            }
            _ => i += 1,
        }
    }
    out
}

fn names(m: &BTreeMap<String, usize>) -> BTreeSet<String> {
    m.keys().cloned().collect()
}

#[test]
fn every_language_has_every_domain_and_valid_files() {
    for lang in LANGUAGES {
        for dom in domains() {
            let path = locales().join(lang).join(format!("{dom}.po"));
            assert!(path.exists(), "fichier manquant : {}", path.display());
            let po = read(&path);
            let (n, expr) = po.plural_forms().unwrap_or_else(|| panic!("{lang}/{dom} : en-tête Plural-Forms absent"));
            assert!(n >= 1, "{lang}/{dom} : nplurals invalide");
            halcyon_i18n::plural::parse(&expr).unwrap_or_else(|e| panic!("{lang}/{dom} : Plural-Forms : {e}"));
            for h in ["X-Halcyon-Native-Name", "X-Halcyon-Thousands-Separator", "X-Halcyon-Decimal-Separator"] {
                assert!(po.headers.get(h).is_some_and(|v| !v.is_empty()), "{lang}/{dom} : en-tête {h} absent");
            }
        }
    }
}

#[test]
fn no_missing_empty_fuzzy_or_obsolete_entries() {
    let mut problems = Vec::new();
    for dom in domains() {
        let template = read(&locales().join("templates").join(format!("{dom}.pot")));
        let wanted: BTreeSet<Key> = template.entries.iter().map(key).collect();
        assert_eq!(wanted.len(), template.entries.len(), "{dom}.pot : clé en double");
        for lang in LANGUAGES {
            let po = read(&locales().join(lang).join(format!("{dom}.po")));
            let mut seen = BTreeSet::new();
            for e in &po.entries {
                let k = key(e);
                if !seen.insert(k.clone()) {
                    problems.push(format!("{lang}/{dom} : en double : {}", show(&k)));
                }
                if !wanted.contains(&k) {
                    problems.push(format!("{lang}/{dom} : clé absente du modèle : {}", show(&k)));
                }
                if e.fuzzy {
                    problems.push(format!("{lang}/{dom} : à revoir (fuzzy) : {}", show(&k)));
                }
                if e.strs.is_empty() || e.strs.iter().any(|s| s.trim().is_empty()) {
                    problems.push(format!("{lang}/{dom} : traduction vide : {}", show(&k)));
                }
            }
            for k in wanted.difference(&seen) {
                problems.push(format!("{lang}/{dom} : clé manquante : {}", show(k)));
            }
        }
    }
    assert!(problems.is_empty(), "{} problème(s) :\n{}", problems.len(), problems.join("\n"));
}

#[test]
fn variables_and_plural_forms_match_the_source() {
    let mut problems = Vec::new();
    for lang in LANGUAGES {
        for dom in domains() {
            let po = read(&locales().join(lang).join(format!("{dom}.po")));
            let (nplurals, _) = po.plural_forms().unwrap();
            for e in &po.entries {
                let k = show(&key(e));
                let src = placeholders(&e.id);
                match &e.id_plural {
                    None => {
                        if e.strs.len() != 1 {
                            problems.push(format!("{lang}/{dom} : {} formes pour un texte simple : {k}", e.strs.len()));
                        }
                        if let Some(t) = e.strs.first() {
                            if placeholders(t) != src {
                                problems.push(format!("{lang}/{dom} : variables {:?} ≠ source {:?} : {k}", placeholders(t), src));
                            }
                        }
                    }
                    Some(plural) => {
                        if e.strs.len() != nplurals {
                            problems.push(format!("{lang}/{dom} : {} formes au lieu de {nplurals} : {k}", e.strs.len()));
                        }
                        let all: BTreeSet<String> = names(&src).union(&names(&placeholders(plural))).cloned().collect();
                        for (i, t) in e.strs.iter().enumerate() {
                            let got = names(&placeholders(t));
                            // une forme peut omettre le nombre (« un jeu »), jamais une autre variable ni en inventer
                            let missing: Vec<_> = all.difference(&got).filter(|v| *v != "n" && *v != "0").collect();
                            let extra: Vec<_> = got.difference(&all).collect();
                            if !missing.is_empty() || !extra.is_empty() {
                                problems.push(format!("{lang}/{dom} : forme {i} : manque {missing:?}, en trop {extra:?} : {k}"));
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(problems.is_empty(), "{} problème(s) :\n{}", problems.len(), problems.join("\n"));
}

#[test]
fn translations_keep_leading_and_trailing_spaces() {
    let mut problems = Vec::new();
    for lang in LANGUAGES {
        for dom in domains() {
            for e in read(&locales().join(lang).join(format!("{dom}.po"))).entries {
                let edge = |s: &str| (s.starts_with([' ', '\n']), s.ends_with([' ', '\n']));
                for t in &e.strs {
                    if edge(t) != edge(&e.id) {
                        problems.push(format!("{lang}/{dom} : espaces de début ou de fin différents : {:?} → {t:?}", e.id));
                    }
                }
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// Les catalogues fusionnés (comme dans le binaire) se chargent, et chaque langue
/// donne une vraie traduction (pas le texte français) pour les textes courants.
#[test]
fn runtime_catalogs_load_and_switch() {
    let mut merged: Vec<(String, String)> = Vec::new();
    for lang in LANGUAGES {
        let mut text = String::new();
        for (i, dom) in domains().iter().enumerate() {
            // Sous Windows, Git peut extraire les .po en CRLF : on normalise avant de
            // découper sur la ligne vide qui sépare l'en-tête des entrées.
            let body = std::fs::read_to_string(locales().join(lang).join(format!("{dom}.po")))
                .unwrap()
                .replace("\r\n", "\n");
            // en-tête du premier fichier seulement
            let body = if i == 0 { body.as_str() } else { body.split_once("\n\n").map(|x| x.1).unwrap_or("") };
            text.push_str(body);
            text.push_str("\n\n");
        }
        merged.push((lang.to_string(), text));
    }
    let refs: Vec<(&str, &str)> = merged.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    halcyon_i18n::register(&refs).expect("catalogues chargeables");
    let codes: Vec<String> = halcyon_i18n::languages().into_iter().map(|l| l.code).collect();
    assert_eq!(codes, LANGUAGES.map(String::from).to_vec());
    let names: BTreeSet<String> = halcyon_i18n::languages().into_iter().map(|l| l.native_name).collect();
    assert_eq!(names.len(), 6, "noms de langue distincts");

    let expected = [
        ("fr-FR", "Paramètres", "Jouer"),
        ("en-US", "Settings", "Play"),
        ("de-DE", "Einstellungen", "Spielen"),
        ("es-ES", "Ajustes", "Jugar"),
        ("it-IT", "Impostazioni", "Gioca"),
        ("pt-PT", "Definições", "Jogar"),
    ];
    for (lang, settings, play) in expected {
        assert!(halcyon_i18n::set_language(lang));
        assert_eq!(halcyon_i18n::language(), lang);
        assert_eq!(halcyon_i18n::t("Paramètres"), settings, "{lang}");
        assert_eq!(halcyon_i18n::t("Jouer"), play, "{lang}");
    }
    assert!(!halcyon_i18n::set_language("ja-JP"), "langue indisponible refusée");
    assert_eq!(halcyon_i18n::language(), "pt-PT", "langue inchangée après un refus");
}

#[test]
fn placeholder_parser() {
    let p = placeholders("{n} jeux, {{littéral}} {} et {}");
    assert_eq!(p.keys().cloned().collect::<Vec<_>>(), ["0", "1", "n"]);
}

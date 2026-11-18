//! Assemblage des catalogues à la compilation (utilisé par les `build.rs` du launcher et de
//! l'installateur, en dépendance de compilation).
//!
//! Pour chaque langue de `locales/` : les fichiers de domaine retenus sont fusionnés en un
//! seul catalogue `<OUT_DIR>/locales/<langue>/LC_MESSAGES/<paquet>.po` — l'emplacement que
//! lisent les traductions intégrées de Slint — et chaque traduction manquante est remplacée
//! par l'anglais. Un fichier `catalogs.rs` liste ces catalogues pour le Rust
//! (`include_str!`), qui les enregistre avec [`crate::register`].

use crate::po::{escape, Entry, PoFile};
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// Domaines retenus pour un binaire.
pub enum Domains<'a> {
    /// Tous les fichiers sauf ceux-ci (launcher : tout sauf `installer`).
    AllExcept(&'a [&'a str]),
    /// Seulement ceux-ci (installateur : `installer`, `formats`).
    Only(&'a [&'a str]),
}

impl Domains<'_> {
    fn keeps(&self, d: &str) -> bool {
        match self {
            Domains::AllExcept(x) => !x.contains(&d),
            Domains::Only(x) => x.contains(&d),
        }
    }
}

fn load(dir: &Path, domains: &Domains) -> Result<(BTreeMap<String, String>, Vec<Entry>), String> {
    let mut headers = BTreeMap::new();
    let mut entries: Vec<Entry> = Vec::new();
    let mut seen: HashMap<(Option<String>, String), usize> = HashMap::new();
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("{} : {e}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "po"))
        .filter(|p| p.file_stem().and_then(|s| s.to_str()).is_some_and(|s| domains.keeps(s)))
        .collect();
    files.sort();
    for f in files {
        println!("cargo:rerun-if-changed={}", f.display());
        let text = std::fs::read_to_string(&f).map_err(|e| format!("{} : {e}", f.display()))?;
        let po = PoFile::parse(&text).map_err(|e| format!("{} : {e}", f.display()))?;
        for (k, v) in po.headers {
            if k != "X-Halcyon-Domain" {
                headers.entry(k).or_insert(v);
            }
        }
        for e in po.entries {
            let key = (e.context.clone(), e.id.clone());
            match seen.get(&key) {
                // un texte présent dans deux domaines : la première traduction non vide l'emporte
                Some(&i) => {
                    if entries[i].strs.iter().any(String::is_empty) && e.strs.iter().all(|s| !s.is_empty()) {
                        entries[i] = e;
                    }
                }
                None => {
                    seen.insert(key, entries.len());
                    entries.push(e);
                }
            }
        }
    }
    Ok((headers, entries))
}

fn write(path: &Path, headers: &BTreeMap<String, String>, entries: &[Entry]) -> Result<(), String> {
    let mut s = String::from("msgid \"\"\nmsgstr \"\"\n");
    for (k, v) in headers {
        let _ = writeln!(s, "{}", escape(&format!("{k}: {v}\n")));
    }
    for e in entries {
        s.push('\n');
        if let Some(c) = &e.context {
            let _ = writeln!(s, "msgctxt {}", escape(c));
        }
        let _ = writeln!(s, "msgid {}", escape(&e.id));
        match &e.id_plural {
            Some(p) => {
                let _ = writeln!(s, "msgid_plural {}", escape(p));
                for (i, t) in e.strs.iter().enumerate() {
                    let _ = writeln!(s, "msgstr[{i}] {}", escape(t));
                }
            }
            None => {
                let _ = writeln!(s, "msgstr {}", escape(e.strs.first().map(String::as_str).unwrap_or("")));
            }
        }
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, s).map_err(|e| format!("{} : {e}", path.display()))
}

fn nplurals(headers: &BTreeMap<String, String>) -> usize {
    headers
        .get("Plural-Forms")
        .and_then(|p| p.split(';').find_map(|x| x.trim().strip_prefix("nplurals=").and_then(|v| v.trim().parse().ok())))
        .unwrap_or(2)
}

/// Assemble les catalogues de `locales_dir` pour le paquet `package` (nom du domaine Slint).
/// Renvoie le dossier à passer à `slint_build::CompilerConfiguration::with_bundled_translations`.
pub fn assemble(locales_dir: &Path, package: &str, domains: Domains) -> Result<PathBuf, String> {
    let out = PathBuf::from(std::env::var("OUT_DIR").map_err(|e| e.to_string())?).join("locales");
    println!("cargo:rerun-if-changed={}", locales_dir.display());
    let mut langs: Vec<String> = std::fs::read_dir(locales_dir)
        .map_err(|e| format!("{} : {e}", locales_dir.display()))?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().to_str().map(String::from))
        .filter(|n| n != "templates")
        .collect();
    langs.sort();

    let (_, fallback) = load(&locales_dir.join(crate::FALLBACK), &domains)?;
    let fallback: HashMap<(Option<String>, String), Vec<String>> =
        fallback.into_iter().map(|e| ((e.context, e.id), e.strs)).collect();

    let mut rs = String::from("/// Catalogues intégrés : (code, contenu .po). Généré par halcyon_i18n::build.\npub static CATALOGS: &[(&str, &str)] = &[\n");
    for lang in &langs {
        let (headers, mut entries) = load(&locales_dir.join(lang), &domains)?;
        let n = nplurals(&headers);
        if lang != crate::REFERENCE {
            for e in &mut entries {
                if e.strs.iter().any(String::is_empty) {
                    if let Some(f) = fallback.get(&(e.context.clone(), e.id.clone())) {
                        if f.iter().all(|s| !s.is_empty()) {
                            let last = f.last().cloned().unwrap_or_default();
                            e.strs = (0..if e.id_plural.is_some() { n } else { 1 })
                                .map(|i| f.get(i).cloned().unwrap_or_else(|| last.clone()))
                                .collect();
                        }
                    }
                }
            }
        }
        let path = out.join(lang).join("LC_MESSAGES").join(format!("{package}.po"));
        write(&path, &headers, &entries)?;
        let _ = writeln!(rs, "    ({lang:?}, include_str!({:?})),", path.display().to_string());
    }
    rs.push_str("];\n");
    std::fs::write(out.join("catalogs.rs"), rs).map_err(|e| e.to_string())?;
    Ok(out)
}

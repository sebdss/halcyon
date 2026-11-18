//! Versions SemVer 2.0 strictes (`1.1.0`, `1.1.0-beta.2`).
//!
//! Contrairement à `halcyon_core::util::version_newer` (tolérant, utilisé pour les jeux et
//! les extensions), HUS refuse toute version mal formée : une décision d'installation ne
//! repose jamais sur une version devinée.

use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Ident {
    Num(u64),
    Alpha(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pre: Vec<Ident>,
}

impl Version {
    pub fn new(major: u64, minor: u64, patch: u64) -> Self {
        Version { major, minor, patch, pre: Vec::new() }
    }

    /// Version de prépublication (`-alpha.1`, `-beta.2`, `-rc.1`).
    pub fn is_prerelease(&self) -> bool {
        !self.pre.is_empty()
    }

    /// Version de ce binaire.
    pub fn current() -> Version {
        env!("CARGO_PKG_VERSION").parse().expect("version du paquet conforme à SemVer")
    }
}

fn numeric(s: &str) -> Result<u64, String> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return Err(tr!("« {s} » n'est pas un nombre", s = s));
    }
    if s.len() > 1 && s.starts_with('0') {
        return Err(tr!("« {s} » : zéro initial interdit", s = s));
    }
    s.parse().map_err(|_| tr!("« {s} » : nombre trop grand", s = s))
}

fn ident_ok(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

impl FromStr for Version {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, String> {
        let t = text.trim();
        let t = t.strip_prefix('v').or_else(|| t.strip_prefix('V')).unwrap_or(t);
        if t.len() > 128 {
            return Err(tr!("version trop longue").into());
        }
        let (rest, build) = match t.split_once('+') {
            Some((a, b)) => (a, Some(b)),
            None => (t, None),
        };
        if let Some(b) = build {
            if !b.split('.').all(ident_ok) {
                return Err(tr!("« {text} » : métadonnées de compilation invalides", text = text));
            }
        }
        let (core, pre) = match rest.split_once('-') {
            Some((a, b)) => (a, Some(b)),
            None => (rest, None),
        };
        let parts: Vec<&str> = core.split('.').collect();
        if parts.len() != 3 {
            return Err(tr!("« {text} » : trois numéros attendus (majeur.mineur.correctif)", text = text));
        }
        let mut v = Version::new(numeric(parts[0])?, numeric(parts[1])?, numeric(parts[2])?);
        if let Some(p) = pre {
            for id in p.split('.') {
                if !ident_ok(id) {
                    return Err(tr!("« {text} » : prépublication invalide", text = text));
                }
                v.pre.push(if id.bytes().all(|b| b.is_ascii_digit()) { Ident::Num(numeric(id)?) } else { Ident::Alpha(id.to_string()) });
            }
        }
        Ok(v)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        for (i, id) in self.pre.iter().enumerate() {
            f.write_str(if i == 0 { "-" } else { "." })?;
            match id {
                Ident::Num(n) => write!(f, "{n}")?,
                Ident::Alpha(s) => f.write_str(s)?,
            }
        }
        Ok(())
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch)
            .cmp(&(other.major, other.minor, other.patch))
            .then_with(|| match (self.pre.is_empty(), other.pre.is_empty()) {
                (true, true) => Ordering::Equal,
                // Une prépublication précède la version finale.
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                (false, false) => {
                    for (a, b) in self.pre.iter().zip(&other.pre) {
                        let o = match (a, b) {
                            (Ident::Num(x), Ident::Num(y)) => x.cmp(y),
                            (Ident::Num(_), Ident::Alpha(_)) => Ordering::Less,
                            (Ident::Alpha(_), Ident::Num(_)) => Ordering::Greater,
                            (Ident::Alpha(x), Ident::Alpha(y)) => x.cmp(y),
                        };
                        if o != Ordering::Equal {
                            return o;
                        }
                    }
                    self.pre.len().cmp(&other.pre.len())
                }
            })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Version {
        s.parse().unwrap()
    }

    #[test]
    fn semver_precedence() {
        // Ordre de la spécification SemVer 2.0 (§11).
        let order = ["1.0.0-alpha", "1.0.0-alpha.1", "1.0.0-alpha.beta", "1.0.0-beta", "1.0.0-beta.2", "1.0.0-beta.11", "1.0.0-rc.1", "1.0.0", "1.0.1", "1.1.0-beta.1", "1.1.0-beta.2", "1.1.0", "2.0.0"];
        for w in order.windows(2) {
            assert!(v(w[0]) < v(w[1]), "{} < {}", w[0], w[1]);
        }
        assert_eq!(v("v1.2.3"), v("1.2.3"));
        assert_eq!(v("1.2.3+build.5"), v("1.2.3"));
        assert_eq!(v("1.1.0-beta.2").to_string(), "1.1.0-beta.2");
        assert!(v("1.1.0-beta.1").is_prerelease() && !v("0.5.1").is_prerelease());
        assert!(Version::current() >= v("1.0.0-alpha.1"));
    }

    #[test]
    fn rejects_malformed() {
        for bad in ["", "1", "1.2", "1.2.3.4", "01.2.3", "1.2.3-", "1.2.3-beta..1", "1.2.3-bêta", "1.2.3-beta.01", "x.y.z", "1.2.3 ", "1.2.3+"] {
            if bad == "1.2.3 " {
                assert!(bad.parse::<Version>().is_ok(), "espaces tolérés autour");
                continue;
            }
            assert!(bad.parse::<Version>().is_err(), "« {bad} » doit être refusée");
        }
    }
}

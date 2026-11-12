//! Parseur du format KeyValues texte de Valve (`.vdf`, `.acf`).
//!
//! ```text
//! "AppState"
//! {
//!     "appid"   "620"
//!     "name"    "Portal 2"
//! }
//! ```
//! Clés insensibles à la casse (Steam n'est pas cohérent : `LastPlayed` / `lastplayed`),
//! commentaires `//` et conditions `[$WIN32]` ignorés.

use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub enum Vdf {
    Str(String),
    Obj(Vec<(String, Vdf)>),
}

#[derive(Debug, PartialEq)]
pub struct VdfError(pub String);

impl fmt::Display for VdfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "VDF invalide : {}", self.0)
    }
}

impl std::error::Error for VdfError {}

impl Vdf {
    /// Analyse un document ; renvoie l'objet racine (toutes les paires de premier niveau).
    pub fn parse(text: &str) -> Result<Vdf, VdfError> {
        let mut p = Parser { s: text.as_bytes(), i: 0 };
        let root = p.parse_pairs(false)?;
        Ok(Vdf::Obj(root))
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Vdf::Str(s) => Some(s),
            Vdf::Obj(_) => None,
        }
    }

    /// Valeur d'une clé (insensible à la casse), si cet élément est un objet.
    pub fn get(&self, key: &str) -> Option<&Vdf> {
        match self {
            Vdf::Obj(pairs) => pairs.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v),
            Vdf::Str(_) => None,
        }
    }

    /// Accès par chemin : `v.path(&["UserLocalConfigStore", "Software", "Valve"])`.
    pub fn path(&self, keys: &[&str]) -> Option<&Vdf> {
        keys.iter().try_fold(self, |v, k| v.get(k))
    }

    pub fn get_str(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(Vdf::as_str)
    }

    pub fn get_u64(&self, key: &str) -> Option<u64> {
        self.get_str(key).and_then(|s| s.trim().parse().ok())
    }

    pub fn entries(&self) -> &[(String, Vdf)] {
        match self {
            Vdf::Obj(pairs) => pairs,
            Vdf::Str(_) => &[],
        }
    }
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn skip_ws(&mut self) {
        loop {
            while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
                self.i += 1;
            }
            if self.s[self.i..].starts_with(b"//") {
                while self.i < self.s.len() && self.s[self.i] != b'\n' {
                    self.i += 1;
                }
            } else if self.i < self.s.len() && self.s[self.i] == b'[' {
                // Condition de plateforme : [$WIN32] — ignorée.
                while self.i < self.s.len() && self.s[self.i] != b']' {
                    self.i += 1;
                }
                self.i = (self.i + 1).min(self.s.len());
            } else {
                break;
            }
        }
    }

    fn parse_pairs(&mut self, nested: bool) -> Result<Vec<(String, Vdf)>, VdfError> {
        let mut pairs = Vec::new();
        loop {
            self.skip_ws();
            match self.s.get(self.i) {
                None if nested => return Err(VdfError("accolade fermante manquante".into())),
                None => return Ok(pairs),
                Some(b'}') if nested => {
                    self.i += 1;
                    return Ok(pairs);
                }
                Some(b'}') => return Err(VdfError(format!("accolade inattendue (octet {})", self.i))),
                _ => {}
            }
            let key = self.parse_token()?;
            self.skip_ws();
            let value = match self.s.get(self.i) {
                Some(b'{') => {
                    self.i += 1;
                    Vdf::Obj(self.parse_pairs(true)?)
                }
                Some(_) => Vdf::Str(self.parse_token()?),
                None => return Err(VdfError(format!("valeur manquante pour « {key} »"))),
            };
            pairs.push((key, value));
        }
    }

    fn parse_token(&mut self) -> Result<String, VdfError> {
        if self.s.get(self.i) == Some(&b'"') {
            self.i += 1;
            let mut out = Vec::new();
            while let Some(&c) = self.s.get(self.i) {
                self.i += 1;
                match c {
                    b'"' => return Ok(String::from_utf8_lossy(&out).into_owned()),
                    b'\\' => {
                        let e = self.s.get(self.i).copied().unwrap_or(b'\\');
                        self.i += 1;
                        out.push(match e {
                            b'n' => b'\n',
                            b't' => b'\t',
                            other => other,
                        });
                    }
                    c => out.push(c),
                }
            }
            Err(VdfError("chaîne non terminée".into()))
        } else {
            let start = self.i;
            while let Some(&c) = self.s.get(self.i) {
                if c.is_ascii_whitespace() || c == b'{' || c == b'}' || c == b'"' {
                    break;
                }
                self.i += 1;
            }
            if start == self.i {
                return Err(VdfError(format!("jeton attendu (octet {start})")));
            }
            Ok(String::from_utf8_lossy(&self.s[start..self.i]).into_owned())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_objects_escapes_and_comments() {
        let text = r#"
        // commentaire
        "libraryfolders"
        {
            "0"
            {
                "path"      "C:\\Program Files (x86)\\Steam"
                "apps"  { "620" "123" }
            }
            "1" { "path" "D:\\SteamLibrary" "label" "Say \"hi\"" }
            "cond" "x" [$WIN32]
            unquoted value
        }"#;
        let v = Vdf::parse(text).unwrap();
        let lf = v.get("LIBRARYFOLDERS").unwrap();
        assert_eq!(lf.path(&["0", "path"]).unwrap().as_str(), Some(r"C:\Program Files (x86)\Steam"));
        assert_eq!(lf.path(&["0", "apps"]).unwrap().get_u64("620"), Some(123));
        assert_eq!(lf.path(&["1", "label"]).unwrap().as_str(), Some("Say \"hi\""));
        assert_eq!(lf.get_str("unquoted"), Some("value"));
        assert_eq!(lf.entries().len(), 4);
    }

    #[test]
    fn rejects_truncated_input() {
        assert!(Vdf::parse(r#""a" { "b" "c""#).is_err());
        assert!(Vdf::parse(r#""a" "unterminated"#).is_err());
        assert!(Vdf::parse(r#""a""#).is_err());
    }
}

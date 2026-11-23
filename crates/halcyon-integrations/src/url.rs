//! Encodage des composants d'URL (RFC 3986, caractères non réservés conservés).

pub(crate) fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn encodes_reserved_characters() {
        assert_eq!(super::encode("Résident Evil 4/é"), "R%C3%A9sident%20Evil%204%2F%C3%A9");
        assert_eq!(super::encode("a-b_c.d~e"), "a-b_c.d~e");
    }
}

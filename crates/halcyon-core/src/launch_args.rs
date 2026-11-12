//! Découpage d'une ligne d'arguments saisie par l'utilisateur, selon les règles
//! de `CommandLineToArgvW` (Windows) : espaces comme séparateurs, guillemets
//! doubles pour grouper, `\"` pour un guillemet littéral.

pub fn split_args(input: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut has_token = false;
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                // Compte les antislashs consécutifs : seuls ceux qui précèdent un guillemet sont spéciaux.
                let mut n = 1;
                while chars.peek() == Some(&'\\') {
                    chars.next();
                    n += 1;
                }
                if chars.peek() == Some(&'"') {
                    cur.extend(std::iter::repeat_n('\\', n / 2));
                    if n % 2 == 1 {
                        chars.next();
                        cur.push('"');
                    }
                } else {
                    cur.extend(std::iter::repeat_n('\\', n));
                }
                has_token = true;
            }
            '"' => {
                in_quotes = !in_quotes;
                has_token = true;
            }
            c if c.is_whitespace() && !in_quotes => {
                if has_token {
                    args.push(std::mem::take(&mut cur));
                    has_token = false;
                }
            }
            c => {
                cur.push(c);
                has_token = true;
            }
        }
    }
    if has_token {
        args.push(cur);
    }
    args
}

#[cfg(test)]
mod tests {
    use super::split_args;

    #[test]
    fn splits_like_windows() {
        assert_eq!(split_args(""), Vec::<String>::new());
        assert_eq!(split_args("-windowed  -w 1920"), ["-windowed", "-w", "1920"]);
        assert_eq!(split_args(r#"--path "C:\My Games\save" -x"#), ["--path", r"C:\My Games\save", "-x"]);
        assert_eq!(split_args(r#"a\"b "" c"#), [r#"a"b"#, "", "c"]);
        assert_eq!(split_args(r"C:\dir\ next"), [r"C:\dir\", "next"]);
    }
}

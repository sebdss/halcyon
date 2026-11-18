//! Règles de pluriel gettext (`plural=` de l'en-tête `Plural-Forms`), évaluées sans
//! dépendance : entiers, `n`, opérateurs C (`?: || && == != < > <= >= + - * / %`, `!`).
//! Ajouter une langue aux règles complexes (polonais, russe, arabe…) ne demande donc que
//! son fichier `.po`.

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    N,
    Num(u64),
    Not(Box<Expr>),
    Bin(Op, Box<Expr>, Box<Expr>),
    Cond(Box<Expr>, Box<Expr>, Box<Expr>),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Op {
    Or,
    And,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
}

impl Expr {
    pub fn eval(&self, n: u64) -> u64 {
        match self {
            Expr::N => n,
            Expr::Num(v) => *v,
            Expr::Not(e) => (e.eval(n) == 0) as u64,
            Expr::Cond(c, a, b) => {
                if c.eval(n) != 0 {
                    a.eval(n)
                } else {
                    b.eval(n)
                }
            }
            Expr::Bin(op, a, b) => {
                let (x, y) = (a.eval(n), b.eval(n));
                match op {
                    Op::Or => (x != 0 || y != 0) as u64,
                    Op::And => (x != 0 && y != 0) as u64,
                    Op::Eq => (x == y) as u64,
                    Op::Ne => (x != y) as u64,
                    Op::Lt => (x < y) as u64,
                    Op::Gt => (x > y) as u64,
                    Op::Le => (x <= y) as u64,
                    Op::Ge => (x >= y) as u64,
                    Op::Add => x.wrapping_add(y),
                    Op::Sub => x.wrapping_sub(y),
                    Op::Mul => x.wrapping_mul(y),
                    Op::Div => x.checked_div(y).unwrap_or(0),
                    Op::Rem => x.checked_rem(y).unwrap_or(0),
                }
            }
        }
    }
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn ws(&mut self) {
        while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }
    fn eat(&mut self, tok: &str) -> bool {
        self.ws();
        if self.s[self.i..].starts_with(tok.as_bytes()) {
            self.i += tok.len();
            true
        } else {
            false
        }
    }
    fn cond(&mut self) -> Result<Expr, String> {
        let c = self.binary(0)?;
        if self.eat("?") {
            let a = self.cond()?;
            if !self.eat(":") {
                return Err("« : » attendu".into());
            }
            let b = self.cond()?;
            return Ok(Expr::Cond(Box::new(c), Box::new(a), Box::new(b)));
        }
        Ok(c)
    }
    fn binary(&mut self, level: usize) -> Result<Expr, String> {
        const LEVELS: [&[(&str, Op)]; 6] = [
            &[("||", Op::Or)],
            &[("&&", Op::And)],
            &[("==", Op::Eq), ("!=", Op::Ne)],
            &[("<=", Op::Le), (">=", Op::Ge), ("<", Op::Lt), (">", Op::Gt)],
            &[("+", Op::Add), ("-", Op::Sub)],
            &[("*", Op::Mul), ("/", Op::Div), ("%", Op::Rem)],
        ];
        if level == LEVELS.len() {
            return self.unary();
        }
        let mut lhs = self.binary(level + 1)?;
        'outer: loop {
            for (tok, op) in LEVELS[level] {
                if self.eat(tok) {
                    let rhs = self.binary(level + 1)?;
                    lhs = Expr::Bin(*op, Box::new(lhs), Box::new(rhs));
                    continue 'outer;
                }
            }
            return Ok(lhs);
        }
    }
    fn unary(&mut self) -> Result<Expr, String> {
        self.ws();
        if self.s.get(self.i) == Some(&b'!') && self.s.get(self.i + 1) != Some(&b'=') {
            self.i += 1;
            return Ok(Expr::Not(Box::new(self.unary()?)));
        }
        if self.eat("(") {
            let e = self.cond()?;
            if !self.eat(")") {
                return Err("« ) » attendue".into());
            }
            return Ok(e);
        }
        if self.eat("n") {
            return Ok(Expr::N);
        }
        let start = self.i;
        while self.i < self.s.len() && self.s[self.i].is_ascii_digit() {
            self.i += 1;
        }
        if start == self.i {
            return Err(format!("expression invalide à la position {}", self.i));
        }
        std::str::from_utf8(&self.s[start..self.i]).unwrap().parse().map(Expr::Num).map_err(|e| e.to_string())
    }
}

pub fn parse(src: &str) -> Result<Expr, String> {
    let mut p = Parser { s: src.as_bytes(), i: 0 };
    let e = p.cond()?;
    p.ws();
    if p.i != p.s.len() {
        return Err(format!("texte inattendu : {}", &src[p.i..]));
    }
    Ok(e)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn french_and_english() {
        let fr = parse("(n > 1)").unwrap();
        assert_eq!([0, 1, 2, 5].map(|n| fr.eval(n)), [0, 0, 1, 1]);
        let en = parse("(n != 1)").unwrap();
        assert_eq!([0, 1, 2].map(|n| en.eval(n)), [1, 0, 1]);
    }

    #[test]
    fn polish_three_forms() {
        let pl = parse("(n==1 ? 0 : n%10>=2 && n%10<=4 && (n%100<10 || n%100>=20) ? 1 : 2)").unwrap();
        assert_eq!([1, 2, 4, 5, 12, 22, 25].map(|n| pl.eval(n)), [0, 1, 1, 2, 2, 1, 2]);
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse("n >").is_err());
        assert!(parse("(n").is_err());
        assert!(parse("n ? 1").is_err());
    }
}

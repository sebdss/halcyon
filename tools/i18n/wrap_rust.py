#!/usr/bin/env python3
"""Outil ponctuel de migration : fait passer par tr! les textes français visibles du Rust.

Convertit les formes sûres (littéral `.into()`, `.to_string()`, `String::from`, `format!`,
`bail!`, `anyhow!`, `.context`, `toast`) et liste le reste pour reprise manuelle."""
import re
import sys

FR = re.compile(r"[àâçéèêëîïôûùüÿœÀÉÈÊ]|\b(le|la|les|des|du|un|une|est|pas|aucun|aucune|introuvable|impossible|jeu|jeux|"
                r"fichier|dossier|lancer|erreur|mise|à|en|de|pour|avec|sur|vos|votre|non|sans|dans|lancé)\b", re.I)
STR = r'"((?:[^"\\]|\\.)*)"'
SKIP_LINE = re.compile(r"(tracing::|log::|debug!|info!|warn!|error!|trace!|eprintln!|println!|#\[|assert|tr!|trn!|trc!)")


def split_args(s):
    """Découpe « a, b(c, d), e » au premier niveau ; renvoie (args, reste après la parenthèse fermante)."""
    args, depth, cur, i = [], 0, "", 0
    while i < len(s):
        c = s[i]
        if c == '"':
            m = re.compile(STR).match(s, i)
            cur += m.group(0); i = m.end(); continue
        if c in "([{":
            depth += 1
        elif c in ")]}":
            if depth == 0:
                if cur.strip():
                    args.append(cur.strip())
                return args, s[i:]
            depth -= 1
        elif c == "," and depth == 0:
            args.append(cur.strip()); cur = ""; i += 1; continue
        cur += c; i += 1
    return None, None


def name_for(expr, used):
    e = expr.strip().lstrip("&*")
    for _ in range(3):
        e = re.sub(r"\.(to_string|display|as_str|clone|len|to_owned|to_string_lossy|as_ref|unwrap_or_default|trim)\(\)$", "", e)
        call = re.match(r"^(.*?)([A-Za-z_]\w*)(::<[^>]*>)?\((.*)\)$", e)
        if call:
            inner = call.group(4).strip()
            e = inner.split(",")[0] if inner and re.search(r"[A-Za-z_]", inner) else call.group(2)
    m = re.search(r"([A-Za-z_][A-Za-z0-9_]*)\s*$", e)
    base = m.group(1).lower() if m else "v"
    if base in ("self", "s", "x", "_len"):
        base = "v"
    base = base.lstrip("_") or "v"
    n, k = base, 2
    while n in used:
        n = f"{base}{k}"; k += 1
    used.add(n)
    return n


def convert_format(fmt, args):
    """« {} jeux ({:.1} Go) », [a, b] → (msg avec {noms}, [(nom, expr)])."""
    out, named, used, pos = "", [], set(), 0
    i = 0
    while i < len(fmt):
        c = fmt[i]
        if fmt.startswith("{{", i) or fmt.startswith("}}", i):
            out += c; i += 2; continue
        if c == "{":
            j = fmt.index("}", i)
            inner = fmt[i + 1:j]
            name, _, spec = inner.partition(":")
            if name == "" or name.isdigit():
                idx = pos if name == "" else int(name)
                if name == "":
                    pos += 1
                expr = args[idx]
            else:
                expr = next((a.split("=", 1)[1].strip() for a in args if re.match(rf"\s*{name}\s*=", a) and "==" not in a), name)
            if spec:
                expr = f'format!("{{:{spec}}}", {expr})'
            ident = name if name and not name.isdigit() and not spec and name not in used else None
            if ident:
                used.add(ident)
            else:
                ident = name_for(expr if not spec else (args[idx] if name == "" or name.isdigit() else name), used)
            if not any(n == ident for n, _ in named):
                named.append((ident, expr))
            out += "{" + ident + "}"
            i = j + 1; continue
        out += c; i += 1
    return out, named


def tr_call(msg, named):
    if not named:
        return f'tr!("{msg}")'
    return f'tr!("{msg}", ' + ", ".join(f"{n} = {e}" for n, e in named) + ")"


def process_line(line):
    if SKIP_LINE.search(line) or line.strip().startswith("//"):
        return line
    # format!(...), bail!(...), anyhow!(...) avec littéral français
    def repl_macro(m):
        mac, lit = m.group(1), m.group(2)
        if not FR.search(lit):
            return m.group(0)
        rest = line_ref[0][m.end():]
        args, tail = split_args(rest)
        if args is None:
            return m.group(0)
        if args and args[0] == "":
            args = args[1:]
        msg, named = convert_format(lit, [a for a in args if a])
        call = tr_call(msg, named)
        consumed[0] = len(rest) - len(tail)
        if mac == "format":
            return call
        return f"{mac}!({call}"
    out = ""
    s = line
    while True:
        line_ref = [s]
        consumed = [0]
        m = re.search(r"\b(format|bail|anyhow)!\(\s*" + STR + r"\s*,?", s)
        if not m or not FR.search(m.group(2)):
            if m:
                out += s[:m.end()]; s = s[m.end():]; continue
            out += s; break
        rep = repl_macro(m)
        if rep == m.group(0):
            out += s[:m.end()]; s = s[m.end():]; continue
        after = s[m.end() + consumed[0]:]
        out += s[:m.start()] + rep
        if m.group(1) == "format":
            after = after[1:]  # parenthèse fermante de format!
        s = after
    line = out
    # littéraux simples
    def repl_lit(m):
        lit = m.group(1)
        if not FR.search(lit) or "{" in lit:
            return m.group(0)
        return f'tr!("{lit}")'
    line = re.sub(r'(?<![!\w])' + STR + r'\.(?:to_string|to_owned)\(\)', repl_lit, line)
    line = re.sub(r'String::from\(' + STR + r'\)', lambda m: repl_lit(m) if FR.search(m.group(1)) and "{" not in m.group(1) else m.group(0), line)
    line = re.sub(r'(?<![!\w(])' + STR + r'\.into\(\)', lambda m: (f'tr!("{m.group(1)}").into()' if FR.search(m.group(1)) and "{" not in m.group(1) else m.group(0)), line)
    line = re.sub(r'(\.context\(|toast\(|ok_or\(|ok_or_else\(\|\| |expect_err\()' + STR,
                  lambda m: m.group(1) + (f'tr!("{m.group(2)}")' if FR.search(m.group(2)) and "{" not in m.group(2) else m.group(0)[len(m.group(1)):]), line)
    return line


def process(path):
    src = open(path, encoding="utf-8").read()
    cut = src.find("#[cfg(test)]")
    head, tail = (src[:cut], src[cut:]) if cut >= 0 else (src, "")
    lines = head.split("\n")
    new = [process_line(l) for l in lines]
    res = "\n".join(new) + tail
    if res != src:
        open(path, "w", encoding="utf-8").write(res)
    left = []
    for n, l in enumerate(new, 1):
        if SKIP_LINE.search(l.replace("tr!(", "").replace("trn!(", "")) or l.strip().startswith("//"):
            continue
        code = re.sub(r'tr!\("(?:[^"\\]|\\.)*"', "", l)
        for m in re.finditer(r'(?<![\w#])' + STR, code):
            if len(m.group(1)) >= 3 and FR.search(m.group(1)):
                left.append((n, l.strip()))
                break
    return left


if __name__ == "__main__":
    total = 0
    for p in sys.argv[1:]:
        for n, l in process(p):
            total += 1
            print(f"{p}:{n}: {l[:220]}")
    print("à reprendre :", total, file=sys.stderr)

#!/usr/bin/env python3
"""Deuxième passe de migration : formes restantes courantes."""
import re, sys
sys.path.insert(0, __import__('os').path.dirname(__file__))
from wrap_rust import FR, STR, convert_format, tr_call, split_args, SKIP_LINE

def fix(line):
    if SKIP_LINE.search(line) or line.strip().startswith("//") or "expect(" in line:
        return line
    # "lit".into() partout
    line = re.sub(r'(?<![!\w])' + STR + r'\.into\(\)', lambda m: f'tr!("{m.group(1)}").into()' if FR.search(m.group(1)) and "{" not in m.group(1) else m.group(0), line)
    # fonctions d'erreur / progression qui prennent un &str
    line = re.sub(r'(io::Error::other\(|Error::other\(|io_err\(|progress\(|StorageError::Conflict\(|VdfError\()' + STR,
                  lambda m: (m.group(1) + ('' if 'other(' in m.group(1) else '&') + f'tr!("{m.group(2)}")') if FR.search(m.group(2)) and "{" not in m.group(2) else m.group(0), line)
    # write!(f, "texte {x}", ...) dans les Display
    m = re.search(r'write!\(\s*f\s*,\s*' + STR, line)
    if m and FR.search(m.group(1)):
        rest = line[m.end():]
        args, tail = split_args(rest)
        if args is not None:
            args = [a for a in args if a]
            msg, named = convert_format(m.group(1), args)
            line = line[:m.start()] + 'write!(f, "{}", ' + tr_call(msg, named) + tail
    return line

for p in sys.argv[1:]:
    src = open(p, encoding="utf-8").read()
    cut = src.find("#[cfg(test)]")
    head, tail = (src[:cut], src[cut:]) if cut >= 0 else (src, "")
    new = "\n".join(fix(l) for l in head.split("\n")) + tail
    if new != src:
        open(p, "w", encoding="utf-8").write(new)

#!/usr/bin/env python3
"""Troisième passe : macros format!/bail!/anyhow! dont le texte est sur une autre ligne."""
import re, sys, os
sys.path.insert(0, os.path.dirname(__file__))
from wrap_rust import FR, STR, convert_format, tr_call, split_args

LOGS = re.compile(r"(tracing::\w+|log::\w+|debug|info|warn|error|trace|eprintln|println|panic|unreachable|expect)!?\s*\(\s*$")

def process(text):
    out, pos = [], 0
    for m in re.finditer(r'\b(format|bail|anyhow)!\(\s*\n\s*' + STR + r'\s*,?', text):
        if m.start() < pos:
            continue
        lit = m.group(2)
        before = text[max(0, m.start() - 40):m.start()]
        if not FR.search(lit) or LOGS.search(before) or re.search(r"(debug|info|warn|error|trace)!\(\s*$", before):
            continue
        args, tail = split_args(text[m.end():])
        if args is None:
            continue
        args = [a for a in args if a]
        msg, named = convert_format(lit, args)
        call = tr_call(msg, named)
        end = len(text) - len(tail)
        rep = call if m.group(1) == "format" else f"{m.group(1)}!({call}"
        if m.group(1) == "format":
            end += 1  # parenthèse fermante
        out.append(text[pos:m.start()]); out.append(rep); pos = end
    out.append(text[pos:])
    return "".join(out)

for p in sys.argv[1:]:
    src = open(p, encoding="utf-8").read()
    cut = src.find("#[cfg(test)]")
    head, tail = (src[:cut], src[cut:]) if cut >= 0 else (src, "")
    new = process(head) + tail
    if new != src:
        open(p, "w", encoding="utf-8").write(new)
        print("modifié", p)

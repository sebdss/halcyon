#!/usr/bin/env python3
"""Enveloppe dans tr!() les littéraux listés (fichier:ligne:«texte») ; format!(…) → tr!(…)."""
import re, sys, os
sys.path.insert(0, os.path.dirname(__file__))
from wrap_rust import convert_format, tr_call, split_args, STR
from collections import defaultdict
todo = defaultdict(list)
for raw in open(sys.argv[1], encoding='utf-8'):
    raw = raw.rstrip('\n')
    if not raw.strip() or raw.startswith('#'): continue
    m = re.match(r'(.+?):(\d+): «(.*)»$', raw)
    todo[m.group(1)].append((int(m.group(2)), m.group(3)))
for f, items in todo.items():
    path = f if f.startswith('crates/') else 'crates/' + f
    src = open(path, encoding='utf-8').read()
    starts = [0]
    for l in src.split('\n'): starts.append(starts[-1] + len(l) + 1)
    for ln, lit in sorted(items, key=lambda x: -x[0]):
        a = starts[ln - 1]; b = starts[ln]
        q = '"' + lit + '"'
        k = src.find(q, a, b + 400)
        if k < 0: print('introuvable', f, ln, lit); continue
        before = src[max(0, k - 30):k]
        fm = re.search(r'format!\(\s*$', before)
        if fm:
            fstart = k - (len(before) - fm.start())
            args, tail = split_args(src[k + len(q):])
            args = [x for x in args if x]
            msg, named = convert_format(lit, args)
            end = len(src) - len(tail) + 1
            src = src[:fstart] + tr_call(msg, named) + src[end:]
        else:
            src = src[:k] + f'tr!({q})' + src[k + len(q):]
    open(path, 'w', encoding='utf-8').write(src)
    print('ok', path, len(items))

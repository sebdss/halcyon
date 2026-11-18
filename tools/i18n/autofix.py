#!/usr/bin/env python3
"""Corrige les erreurs de type introduites par tr!() : « expected &str, found String » → &tr!(…),
et applique les suggestions MachineApplicable/MaybeIncorrect de rustc qui ajoutent un emprunt
ou un .into()/.to_string(). Boucle jusqu'à stabilité."""
import json, subprocess, sys, collections
def outer(s):
    while s.get('expansion'):
        s = s['expansion']['span']
    return s
for rnd in range(12):
    p = subprocess.run(['cargo', 'check', '--workspace', '--tests', '--message-format=json'] + sys.argv[1:], capture_output=True, text=True)
    edits = collections.defaultdict(list)
    errs = 0
    for line in p.stdout.splitlines():
        try: msg = json.loads(line)
        except Exception: continue
        if msg.get('reason') != 'compiler-message': continue
        m = msg['message']
        if m['level'] != 'error': continue
        errs += 1
        def walk(x):
            for c in x.get('children', []):
                for s in c.get('spans', []):
                    if s.get('expansion') or 'halcyon-i18n/src' in s['file_name']:
                        continue
                    if s.get('suggested_replacement') is not None and s.get('suggestion_applicability') in ('MachineApplicable', 'MaybeIncorrect'):
                        rep = s['suggested_replacement']
                        if rep.startswith('&') or rep.endswith('.into()') or rep.endswith('.to_string()') or rep.endswith('.as_str()') or rep == '&' or rep.startswith('*'):
                            edits[s['file_name']].append((s['byte_start'], s['byte_end'], rep))
                            return True
                if walk(c): return True
            return False
        if not walk(m):
            for s in m.get('spans', []):
                lbl = s.get('label') or ''
                if s.get('is_primary') and 'expected `&str`, found `String`' in lbl:
                    s = outer(s)
                    edits[s['file_name']].append((s['byte_start'], s['byte_start'], '&'))
    if errs == 0:
        print('compilation OK'); break
    if not edits:
        print(f'{errs} erreurs sans correction automatique'); print(p.stderr[-3000:]); break
    for f, es in edits.items():
        b = open(f, 'rb').read()
        seen = set()
        for s, e, r in sorted(set(es), key=lambda x: -x[0]):
            if (s, e) in seen: continue
            seen.add((s, e))
            b = b[:s] + r.encode() + b[e:]
        open(f, 'wb').write(b)
    print('tour', rnd, 'corrections', sum(len(v) for v in edits.values()))

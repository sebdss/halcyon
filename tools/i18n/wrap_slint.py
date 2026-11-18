#!/usr/bin/env python3
"""Outil ponctuel de migration : entoure de @tr(...) les textes visibles des fichiers .slint.
Les cas ambigus (concaténations, comparaisons) sont listés pour reprise manuelle."""
import re, sys, glob

PROPER = {"Steam", "Epic Games", "Epic", "GOG", "Minecraft", "Halcyon", "Modrinth", "CurseForge", "Fabric", "Quilt",
          "Forge", "NeoForge", "RetroAchievements", "Heroic", "Legendary", "Nexus Mods", "Sunshine", "Moonlight",
          "MangoHud", "PresentMon", "Archivo", "Archivo Expanded", "Consolas", "MC", "S", "M", "L", "Release",
          "Halcyon ", "GitHub", "Steam Web API", "Xbox", "PlayStation", "Discord", "Wine", "Proton", "umu-run", "XInput", "evdev"}
SKIP_PROPS = re.compile(r'(font-family|default-font-family|form-args|tint|display|font)\s*:\s*[^;]*$')
LIT = re.compile(r'"((?:[^"\\]|\\.)*)"')

def visible(t):
    if not re.search(r'[A-Za-zÀ-ÿ]', t): return False
    if re.fullmatch(r'[a-z0-9_\-\.:]+', t): return False
    if t.endswith(('.svg', '.png', '.ttf', '.slint', '.otf')): return False
    if t in PROPER: return False
    return True

def process(path):
    out = []; manual = []
    for n, line in enumerate(open(path, encoding='utf-8'), 1):
        code, sep, comment = line.partition('//') if '"' not in line.split('//')[0] or line.find('//') > line.rfind('"') else (line, '', '')
        if re.match(r'\s*import\b', code) or ' from "' in code:
            out.append(line); continue
        res = []; last = 0; changed = False
        for m in LIT.finditer(code):
            t = m.group(1)
            before = code[:m.start()]; after = code[m.end():]
            if before.rstrip().endswith('@tr(') or not visible(t):
                continue
            if re.search(r'(==|!=)\s*$', before) or re.match(r'\s*(==|!=)', after):
                continue
            if SKIP_PROPS.search(before):
                continue
            res.append(code[last:m.start()]); res.append('@tr(' + m.group(0) + ')'); last = m.end(); changed = True
        if changed:
            res.append(code[last:])
            new = ''.join(res) + (sep + comment if sep else '')
            if re.search(r'\)\s*\+|\+\s*@tr\(|"\s*\+|\+\s*"', new.split('//')[0]):
                manual.append((n, new.rstrip()))
            out.append(new)
        else:
            out.append(line)
    open(path, 'w', encoding='utf-8').write(''.join(out))
    return manual

if __name__ == '__main__':
    total = 0
    for f in sys.argv[1:]:
        for n, l in process(f):
            total += 1
            print(f'{f}:{n}: {l.strip()}')
    print('à reprendre :', total, file=sys.stderr)

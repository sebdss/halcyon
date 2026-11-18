#!/usr/bin/env python3
"""Migration : fonctions `fn x(..) -> &'static str` qui renvoient des libellés → `-> String` traduits.
Usage : label_fns.py fichier fn1 fn2 ..."""
import re, sys
FR = re.compile(r"[àâçéèêëîïôûùüÿœÀÉÈÊ]|\b(le|la|les|des|du|un|une|non|sans|jour|jeu|jeux|ajout|aucun|aucune|suivi|corrigé|plateforme|moteur|basse|haute|normale|nom|rare|commun|manuel|personnalisé|standalone)\b", re.I)
path, names = sys.argv[1], sys.argv[2:]
s = open(path, encoding="utf-8").read()
for name in names * 8:
    m = re.search(rf"fn {name}\b([^{{]*?)->\s*&'static str\s*\{{", s)
    if not m:
        continue
    start = m.end(); depth = 1; i = start
    while depth:
        if s[i] == "{": depth += 1
        elif s[i] == "}": depth -= 1
        i += 1
    body = s[start:i]
    def rep(mm):
        lit = mm.group(2)
        if lit == "":
            return mm.group(1) + "String::new()"
        if FR.search(lit) or (name in ("rarity_label",) ):
            return mm.group(1) + f'tr!("{lit}")'
        return mm.group(1) + f'"{lit}".into()'
    body = re.sub(r'(=>\s*)"((?:[^"\\]|\\.)*)"', rep, body)
    s = s[:m.start()] + f"fn {name}{m.group(1)}-> String {{" + body + s[i:]
open(path, "w", encoding="utf-8").write(s)

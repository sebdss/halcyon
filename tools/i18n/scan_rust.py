"""Liste les littéraux Rust qui ressemblent à du texte visible non traduit."""
import re, glob, sys
STR = re.compile(r'(?<![A-Za-z0-9_#])"((?:[^"\\\n]|\\.)*)"')
SKIP_BEFORE = re.compile(r'(tr|trc|trn|trcn|tr_noop|trc_noop|tr_later|trn_later|debug|info|warn|error|trace|log::\w+|eprintln|println|panic|unreachable|expect|assert\w*|include_\w+|env|concat|cfg|doc|serde|Stopwatch::new|lap|matches|unimplemented|todo)!?\s*\([^()]*$')
WORD = re.compile(r"[A-Za-zÀ-ÿ]{3,}")
FRW = re.compile(r"[éèêàùçôîœÉÈÀ«»]|\b(le|la|les|des|du|de|un|une|est|pas|aucun|aucune|impossible|introuvable|fichier|jeu|jeux|dossier|erreur|échec|sauvegarde|vous|votre|pour|avec|sans|dans|sur|mise|lancer|ouvrir|valide|invalide|manquant|annul)\b", re.I)
out=[]
for p in sorted(glob.glob('crates/*/src/**/*.rs', recursive=True)):
    if 'hus-release' in p or 'halcyon-updater.rs' in p: continue
    src=open(p).read()
    # coupe les modules de test
    cut=re.search(r'#\[cfg\(test\)\]\s*mod\s+\w+', src)
    body=src[:cut.start()] if cut else src
    for ln,line in enumerate(body.split('\n'),1):
        s=line.strip()
        if s.startswith('//') or s.startswith('#['): continue
        for m in STR.finditer(line):
            lit=m.group(1)
            if not FRW.search(lit) or not WORD.search(lit): continue
            before=line[:m.start()]
            if SKIP_BEFORE.search(before): continue
            if '/' in lit and ' ' not in lit: continue
            out.append(f"{p}:{ln}: {s[:160]}")
print('\n'.join(out)); print(len(out), file=sys.stderr)

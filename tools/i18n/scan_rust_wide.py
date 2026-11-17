"""Balayage large : tout littéral qui ressemble à un mot ou une phrase (majuscule initiale ou
espace), hors macros de traduction, journaux et tests. À relire à la main."""
import re, glob, sys
sys.path.insert(0, 'tools/i18n')
STR = re.compile(r'(?<![A-Za-z0-9_#])"((?:[^"\\\n]|\\.)*)"')
TRMAC = re.compile(r'\b(tr|trc|trn|trcn|tr_noop|trc_noop|tr_later|trn_later)!\s*\(')
LOGMAC = re.compile(r'\b(debug|info|warn|error|trace|eprintln|println|panic|unreachable|assert\w*|expect|include_\w+|env|concat|option_env|write|writeln|format_args)!?\s*\(')
allow = set(l.strip() for l in open('tools/i18n/lint-allow.txt', encoding='utf-8') if l.strip() and not l.startswith('#')) if __import__('os').path.exists('tools/i18n/lint-allow.txt') else set()
def looks(lit):
    if lit in allow: return False
    if not re.search(r'[A-Za-zÀ-ÿ]{3,}', lit): return False
    if re.fullmatch(r'[a-z0-9_.:/\\%{}\-+*#=?&<>|$@^~\[\]()]+', lit): return False  # identifiants, chemins, codes
    if re.match(r'^(https?|steam|heroic|file)://', lit): return False
    if re.fullmatch(r'[A-Z0-9_]+', lit): return False
    return bool(re.match(r'^[A-ZÀ-Ü«]', lit) or ' ' in lit.strip())
def mask_calls(src, rx):
    """Positions couvertes par les appels de macro rx (parenthèses équilibrées)."""
    spans=[]
    for m in rx.finditer(src):
        i=m.end(); depth=1; ins=False
        while i < len(src) and depth:
            c=src[i]
            if ins:
                if c=='\\': i+=1
                elif c=='"': ins=False
            elif c=='"': ins=True
            elif c=='(': depth+=1
            elif c==')': depth-=1
            i+=1
        spans.append((m.start(), i))
    return spans
out=[]
for p in sorted(glob.glob('crates/*/src/**/*.rs', recursive=True)):
    if 'hus-release' in p or 'halcyon-updater.rs' in p or 'halcyon-i18n' in p: continue
    src=open(p, encoding='utf-8').read()
    cut=re.search(r'#\[cfg\(test\)\]\s*(pub\s+)?mod\s+\w+', src)
    body=src[:cut.start()] if cut else src
    spans=mask_calls(body, TRMAC)+mask_calls(body, LOGMAC)+mask_calls(body, re.compile(r'\blog::\w+!\s*\('))
    def inside(pos): return any(a<=pos<b for a,b in spans)
    lines=body.split('\n'); off=0; starts=[]
    for l in lines: starts.append(off); off+=len(l)+1
    import bisect
    for m in STR.finditer(body):
        lit=m.group(1)
        if not looks(lit) or inside(m.start()): continue
        ln=bisect.bisect_right(starts, m.start())
        line=lines[ln-1].strip()
        if line.startswith('//') or line.startswith('#[') or line.startswith('///'): continue
        after=body[m.end():m.end()+4]
        if re.match(r'\s*(=>|\|)', after): continue   # motif de match
        out.append(f"{p}:{ln}: «{lit}»  ‖ {line[:140]}")
print('\n'.join(out)); print(len(out), file=sys.stderr)

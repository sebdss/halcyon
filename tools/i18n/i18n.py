#!/usr/bin/env python3
"""Outils de traduction de Halcyon.

  python3 tools/i18n/i18n.py extract        met à jour locales/templates/*.pot et toutes les langues
  python3 tools/i18n/i18n.py status         avancement de chaque langue
  python3 tools/i18n/i18n.py lint           textes visibles oubliés (non passés par @tr / tr!)
  python3 tools/i18n/i18n.py add-language nl-NL "Nederlands" "nplurals=2; plural=(n != 1);" "." ","

Les textes source sont en français et servent de clés. Voir docs/i18n/README.md.
"""
import collections
import glob
import json
import os
import re
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
LOCALES = os.path.join(ROOT, "locales")
TEMPLATES = os.path.join(LOCALES, "templates")
REFERENCE = "fr-FR"
FALLBACK = "en-US"

# Langues initiales : nom natif, règle de pluriel, séparateur de milliers, séparateur décimal.
LANGUAGES = {
    "fr-FR": ("Français", "nplurals=2; plural=(n > 1);", "U+00A0", ","),
    "en-US": ("English", "nplurals=2; plural=(n != 1);", ",", "."),
    "de-DE": ("Deutsch", "nplurals=2; plural=(n != 1);", ".", ","),
    "es-ES": ("Español", "nplurals=2; plural=(n != 1);", ".", ","),
    "it-IT": ("Italiano", "nplurals=2; plural=(n != 1);", ".", ","),
    "pt-PT": ("Português", "nplurals=2; plural=(n != 1);", "U+00A0", ","),
}

# Domaines : premier motif de chemin qui correspond. Un texte du launcher présent dans
# plusieurs domaines va dans « common ». L'installateur a son propre catalogue.
DOMAINS = [
    ("installer", r"crates/halcyon-installer/"),
    ("formats", r"crates/halcyon-core/src/(util|stats)\.rs|crates/halcyon-update/src/download\.rs"),
    ("navigation", r"ui/(app|shell|state|components|theme|types|sound)\.slint|src/(controller|ctl_palette|ctl_themes|main|app)\.rs"),
    ("home", r"ui/pages/home\.slint|src/ctl_home\.rs"),
    ("library", r"ui/pages/(library|smart|collections)\.slint|ui/collections_state\.slint|src/ctl_(library|smart|scan|collections)\.rs|halcyon-core/src/(rules|search|model|collections)\.rs|halcyon-integrations/src/(steam|epic|gog|standalone|manual|scan|artwork|metadata)"),
    ("games", r"ui/pages/game\.slint|ui/uninstall_state\.slint|src/ctl_(detail|game|profiles|mods|tracking|achievements|captures|news|uninstall)\.rs|halcyon-core/src/uninstall\.rs|halcyon-integrations/src/trash|halcyon-core/src/profiles\.rs|halcyon-integrations/src/(mods|launcher|achievements|engines?|news)"),
    ("activity", r"ui/pages/(achievements|stats|journal|assistant)\.slint|src/ctl_(stats|journal|assistant)\.rs|halcyon-core/src/(recommend|export)\.rs|halcyon-storage/src/tracking\.rs"),
    ("tools", r"ui/pages/(downloads|updates|saves|emulators|controllers|remote|plugins)\.slint|src/ctl_(downloads|updates|saves|emulators|controllers|pads|remote|plugins|backup)\.rs|halcyon-integrations/src/(saves|emulators|plugins|remote|sunshine|pads|controllers)|halcyon-update/"),
    ("minecraft", r"ui/pages/minecraft\.slint|src/ctl_minecraft\.rs|crates/halcyon-minecraft/"),
    ("console", r"ui/pages/(bigpicture|shelf)\.slint|ui/console_state\.slint|src/ctl_(bigpicture|shelf)\.rs"),
    ("system", r"ui/pages/(settings|appearance|performance)\.slint|src/ctl_(system|settings|perf|performance|update)\.rs|halcyon-integrations/src/sysmon|crates/halcyon-audio/|crates/halcyon-storage/"),
    ("navigation", r"."),
]
LAUNCHER_ONLY = [d for d, _ in DOMAINS if d != "installer"]


def domain_of(path):
    for d, pat in DOMAINS:
        if re.search(pat, path):
            return d
    return "navigation"


# ------------------------------------------------------------------ lecture des sources
SLINT_STR = r'"((?:[^"\\]|\\.)*)"'


def unescape_slint(s):
    return re.sub(r'\\(.)', lambda m: {"n": "\n", "t": "\t"}.get(m.group(1), m.group(1)), s)


def unescape_rust(s):
    def rep(m):
        c = m.group(1)
        if c.startswith("u{"):
            return chr(int(c[2:-1], 16))
        return {"n": "\n", "t": "\t", "r": "\r", "0": "\0"}.get(c, c)
    return re.sub(r'\\(u\{[0-9a-fA-F]+\}|.)', rep, s)


def strip_line_comments(text):
    out = []
    for line in text.split("\n"):
        # retire « // commentaire » hors des chaînes
        res, in_str, i = [], False, 0
        while i < len(line):
            c = line[i]
            if c == '"' and (i == 0 or line[i - 1] != "\\" or line[i - 2:i] == "\\\\"):
                in_str = not in_str
            if not in_str and line.startswith("//", i):
                break
            res.append(c)
            i += 1
        out.append("".join(res))
    return "\n".join(out)


def slint_messages(path):
    text = strip_line_comments(open(path, encoding="utf-8").read())
    for m in re.finditer(r'@tr\(\s*', text):
        pos = m.end()
        lit = re.compile(SLINT_STR)
        a = lit.match(text, pos)
        if not a:
            continue
        ctx, msgid, plural = None, unescape_slint(a.group(1)), None
        pos = a.end()
        arrow = re.compile(r'\s*=>\s*').match(text, pos)
        if arrow:
            b = lit.match(text, arrow.end())
            ctx, msgid, pos = msgid, unescape_slint(b.group(1)), b.end()
        bar = re.compile(r'\s*\|\s*').match(text, pos)
        if bar:
            c = lit.match(text, bar.end())
            plural = unescape_slint(c.group(1))
        line = text.count("\n", 0, m.start()) + 1
        yield ctx, msgid, plural, line


RUST_STR = r'"((?:[^"\\]|\\.)*)"'


def rust_messages(path):
    src = open(path, encoding="utf-8").read()
    # les modules de test ne contiennent pas de texte affiché
    cut = re.search(r"#\[cfg\(test\)\]\s*(pub(\(\w+\))?\s+)?mod\s+\w+", src)
    if cut:
        src = src[:cut.start()]
    text = strip_line_comments(src)
    for m in re.finditer(r'\b(tr|trc|trn|tr_noop|trc_noop|tr_later|trn_later)!\s*\(', text):
        kind = m.group(1)
        pos = m.end()
        lit = re.compile(r'\s*' + RUST_STR)
        ctx = plural = None
        if kind in ("trn", "trn_later"):
            # premier argument : expression jusqu'à la virgule de premier niveau
            depth, i = 0, pos
            while i < len(text):
                ch = text[i]
                if ch in "([{":
                    depth += 1
                elif ch in ")]}":
                    depth -= 1
                elif ch == '"':
                    i = re.compile(RUST_STR).match(text, i).end() - 1
                elif ch == "," and depth == 0:
                    break
                i += 1
            a = lit.match(text, i + 1)
            b = re.compile(r'\s*,' + r'\s*' + RUST_STR).match(text, a.end())
            msgid, plural = unescape_rust(a.group(1)), unescape_rust(b.group(1))
        else:
            a = lit.match(text, pos)
            if not a:
                continue
            msgid = unescape_rust(a.group(1))
            if kind in ("trc", "trc_noop"):
                b = re.compile(r'\s*,\s*' + RUST_STR).match(text, a.end())
                ctx, msgid = msgid, unescape_rust(b.group(1))
        line = text.count("\n", 0, m.start()) + 1
        yield ctx, msgid, plural, line


def sources():
    files = sorted(glob.glob(os.path.join(ROOT, "crates/**/*.slint"), recursive=True))
    files += sorted(p for p in glob.glob(os.path.join(ROOT, "crates/*/src/**/*.rs"), recursive=True)
                    if "/bin/hus-release" not in p)
    return files


def collect():
    """{(ctx, msgid): {"plural": str|None, "refs": [..], "domains": set}} dans l'ordre d'apparition."""
    msgs = collections.OrderedDict()
    for f in sources():
        rel = os.path.relpath(f, ROOT)
        gen = slint_messages(f) if f.endswith(".slint") else rust_messages(f)
        for ctx, msgid, plural, line in gen:
            if not msgid.strip():
                continue
            key = (ctx, msgid)
            e = msgs.setdefault(key, {"plural": plural, "refs": [], "domains": set()})
            if plural and not e["plural"]:
                e["plural"] = plural
            e["refs"].append(f"{rel}:{line}")
            e["domains"].add(domain_of(rel))
    return msgs


def assign(msgs):
    """Domaine de chaque message. Un message de l'installateur va aussi dans son catalogue."""
    out = collections.defaultdict(collections.OrderedDict)
    for key, e in msgs.items():
        doms = e["domains"]
        if "installer" in doms:
            out["installer"][key] = e
        launcher = doms - {"installer"}
        if not launcher:
            continue
        if "formats" in launcher:
            d = "formats"
        elif len(launcher) > 1:
            d = "common"
        else:
            d = next(iter(launcher))
        out[d][key] = e
    return out


# ------------------------------------------------------------------ fichiers .po
def po_escape(s):
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n").replace("\t", "\\t") + '"'


def po_unescape(s):
    s = s.strip()[1:-1]
    return re.sub(r'\\(.)', lambda m: {"n": "\n", "t": "\t", "r": "\r"}.get(m.group(1), m.group(1)), s)


def read_po(path):
    """Renvoie (en-têtes bruts, {(ctx, msgid): {"plural", "strs", "comments"}})."""
    entries = collections.OrderedDict()
    header = ""
    if not os.path.exists(path):
        return header, entries
    cur, field, comments = {}, None, []

    def flush():
        nonlocal cur, comments, header
        if "msgid" in cur:
            if cur["msgid"] == "" and cur.get("ctx") is None:
                header = cur.get("strs", {}).get(0, "")
            else:
                strs = [cur.get("strs", {}).get(i, "") for i in range(max(cur.get("strs", {0: ""}).keys()) + 1)]
                entries[(cur.get("ctx"), cur["msgid"])] = {"plural": cur.get("plural"), "strs": strs,
                                                           "comments": [c for c in comments if c.startswith("# ")]}
        cur, comments = {}, []

    for raw in open(path, encoding="utf-8"):
        line = raw.strip()
        if not line:
            flush(); field = None; continue
        if line.startswith("#~"):
            continue
        if line.startswith("#"):
            if field and field != "ctx":
                flush(); field = None
            comments.append(line)
            continue
        if line.startswith('"'):
            v = po_unescape(line)
            if field == "ctx": cur["ctx"] += v
            elif field == "id": cur["msgid"] += v
            elif field == "plural": cur["plural"] += v
            elif isinstance(field, int): cur["strs"][field] += v
            continue
        kw, _, val = line.partition(" ")
        if kw == "msgctxt":
            flush(); cur["ctx"] = po_unescape(val); field = "ctx"
        elif kw == "msgid":
            if isinstance(field, int):
                flush()
            cur["msgid"] = po_unescape(val); field = "id"
        elif kw == "msgid_plural":
            cur["plural"] = po_unescape(val); field = "plural"
        elif kw == "msgstr":
            cur.setdefault("strs", {})[0] = po_unescape(val); field = 0
        elif kw.startswith("msgstr["):
            k = int(kw[7:-1]); cur.setdefault("strs", {})[k] = po_unescape(val); field = k
    flush()
    return header, entries


def header_for(lang, domain):
    name, plural, th, dec = LANGUAGES.get(lang) or language_meta(lang)
    return ("Project-Id-Version: Halcyon\n"
            f"Language: {lang}\n"
            "MIME-Version: 1.0\n"
            "Content-Type: text/plain; charset=UTF-8\n"
            "Content-Transfer-Encoding: 8bit\n"
            f"Plural-Forms: {plural}\n"
            f"X-Halcyon-Native-Name: {name}\n"
            f"X-Halcyon-Thousands-Separator: {th}\n"
            f"X-Halcyon-Decimal-Separator: {dec}\n"
            f"X-Halcyon-Domain: {domain}\n")


def language_meta(lang):
    """Métadonnées d'une langue ajoutée après coup : lues dans son fichier common.po."""
    hdr, _ = read_po(os.path.join(LOCALES, lang, "common.po"))
    h = dict(l.split(":", 1) for l in hdr.split("\n") if ":" in l)
    g = lambda k, d: h.get(k, d).strip()
    return (g("X-Halcyon-Native-Name", lang), g("Plural-Forms", "nplurals=2; plural=(n != 1);"),
            g("X-Halcyon-Thousands-Separator", ","), g("X-Halcyon-Decimal-Separator", "."))


def nplurals(lang):
    plural = (LANGUAGES.get(lang) or language_meta(lang))[1]
    return int(re.search(r'nplurals\s*=\s*(\d+)', plural).group(1))


def write_po(path, header, items, template=False):
    lines = ["# Halcyon — catalogue de traduction. Généré par tools/i18n/i18n.py extract ;",
             "# modifier seulement les lignes msgstr.", 'msgid ""', 'msgstr ""']
    for l in header.split("\n"):
        if l:
            lines.append(po_escape(l + "\n"))
    for (ctx, msgid), e in items:
        lines.append("")
        for c in e.get("comments", []):
            lines.append(c)
        refs = e.get("refs", [])
        for i in range(0, len(refs), 3):
            lines.append("#: " + " ".join(refs[i:i + 3]))
        if "{" in msgid:
            lines.append("#, placeholders")
        if ctx is not None:
            lines.append("msgctxt " + po_escape(ctx))
        lines.append("msgid " + po_escape(msgid))
        if e.get("plural"):
            lines.append("msgid_plural " + po_escape(e["plural"]))
            for i, s in enumerate(e["strs"]):
                lines.append(f"msgstr[{i}] " + po_escape(s))
        else:
            lines.append("msgstr " + po_escape(e["strs"][0] if e["strs"] else ""))
    os.makedirs(os.path.dirname(path), exist_ok=True)
    open(path, "w", encoding="utf-8").write("\n".join(lines) + "\n")


def languages():
    return sorted(d for d in os.listdir(LOCALES) if os.path.isdir(os.path.join(LOCALES, d)) and d != "templates")


def extract():
    msgs = collect()
    by_domain = assign(msgs)
    os.makedirs(TEMPLATES, exist_ok=True)
    for f in glob.glob(os.path.join(TEMPLATES, "*.pot")):
        os.remove(f)
    for d, items in sorted(by_domain.items()):
        write_po(os.path.join(TEMPLATES, f"{d}.pot"), header_for(REFERENCE, d).replace(f"Language: {REFERENCE}\n", ""),
                 [(k, {**e, "strs": [""] * (2 if e["plural"] else 1)}) for k, e in items.items()], template=True)
    langs = sorted(set(languages()) | set(LANGUAGES))
    for lang in langs:
        old = {}
        for p in glob.glob(os.path.join(LOCALES, lang, "*.po")):
            _, ents = read_po(p)
            for k, e in ents.items():
                if any(e["strs"]):
                    old.setdefault(k, e)
        n = nplurals(lang)
        for p in glob.glob(os.path.join(LOCALES, lang, "*.po")):
            os.remove(p)
        for d, items in sorted(by_domain.items()):
            out = []
            for k, e in items.items():
                prev = old.get(k)
                if lang == REFERENCE:
                    strs = [k[1], e["plural"]] if e["plural"] else [k[1]]
                elif prev and bool(prev["plural"]) == bool(e["plural"]):
                    strs = prev["strs"]
                    if e["plural"]:
                        strs = (strs + [strs[-1]] * n)[:n]
                else:
                    strs = [""] * (n if e["plural"] else 1)
                out.append((k, {"plural": e["plural"], "refs": e["refs"], "strs": strs,
                                "comments": (prev or {}).get("comments", [])}))
            write_po(os.path.join(LOCALES, lang, f"{d}.po"), header_for(lang, d), out)
    total = sum(len(v) for v in by_domain.values())
    print(f"{len(msgs)} textes, {total} entrées réparties en {len(by_domain)} domaines : "
          + ", ".join(f"{d} {len(v)}" for d, v in sorted(by_domain.items())))


def status():
    for lang in languages():
        done = todo = 0
        for p in glob.glob(os.path.join(LOCALES, lang, "*.po")):
            _, ents = read_po(p)
            for e in ents.values():
                if all(e["strs"]):
                    done += 1
                else:
                    todo += 1
        print(f"{lang}: {done}/{done + todo} traduits" + (f", {todo} manquants" if todo else ""))


# ------------------------------------------------------------------ lint
FR_WORDS = re.compile(r"[àâçéèêëîïôûùüÿœÀÉÈÊ]|\b(le|la|les|des|du|un|une|est|pas|aucun|aucune|introuvable|impossible|"
                      r"jeu|jeux|fichier|dossier|lancer|erreur|mise|à|en|de|pour|avec|sur|vos|votre|ou|et|au|aux|par|dans|sans|ce|cet|cette|ces|vous|nous|ne)\b", re.I)
ALLOW = os.path.join(os.path.dirname(__file__), "lint-allow.txt")


def rust_tokens(text):
    """Lexique minimal de Rust : renvoie (chaînes, texte masqué). Les chaînes sont des
    (début, fin, valeur) ; le texte masqué remplace commentaires, caractères et contenu des
    chaînes par des espaces (les parenthèses y sont alors fiables)."""
    out, masked, i, n = [], list(text), 0, len(text)

    def blank(a, b):
        for k in range(a, b):
            if masked[k] != "\n":
                masked[k] = " "

    while i < n:
        c = text[i]
        if text.startswith("//", i):
            j = text.find("\n", i)
            j = n if j < 0 else j
            blank(i, j)
            i = j
        elif text.startswith("/*", i):
            j = text.find("*/", i + 2)
            j = n if j < 0 else j + 2
            blank(i, j)
            i = j
        elif re.match(r"b?r(#*)\"", text[i:i + 12]) and (i == 0 or not (text[i - 1].isalnum() or text[i - 1] == "_")):
            m = re.match(r"b?r(#*)\"", text[i:])
            close = '"' + m.group(1)
            j = text.find(close, i + m.end())
            j = n if j < 0 else j + len(close)
            out.append((i, j, text[i + m.end():j - len(close)]))
            blank(i + m.end(), j - len(close))
            i = j
        elif c == '"' or (c == "b" and text.startswith('b"', i) and (i == 0 or not text[i - 1].isalnum())):
            k = i + (1 if c == '"' else 2)
            j = k
            while j < n and text[j] != '"':
                j += 2 if text[j] == "\\" else 1
            out.append((i, j + 1, text[k:j]))
            blank(k, j)
            i = j + 1
        elif c == "'":
            m = re.match(r"'(\\(x[0-9a-fA-F]{2}|u\{[0-9a-fA-F]+\}|.)|[^\\'])'", text[i:])
            if m:
                blank(i, i + m.end())
                i += m.end()
            else:
                i += 1  # durée de vie
        else:
            i += 1
    return out, "".join(masked)


def macro_spans(masked, names):
    """Étendues des appels `nom!(…)` dans le texte masqué (parenthèses équilibrées)."""
    spans = []
    for m in re.finditer(r"\b(" + names + r")!\s*[\(\[]", masked):
        i, depth = m.end(), 1
        while i < len(masked) and depth:
            if masked[i] in "([":
                depth += 1
            elif masked[i] in ")]":
                depth -= 1
            i += 1
        spans.append((m.start(), i))
    return spans


def call_spans(masked, pattern):
    """Étendues des appels de fonction ou de méthode `motif(…)` (texte masqué)."""
    spans = []
    for m in re.finditer(pattern + r"\s*\(", masked):
        i, depth = m.end(), 1
        while i < len(masked) and depth:
            if masked[i] in "([":
                depth += 1
            elif masked[i] in ")]":
                depth -= 1
            i += 1
        spans.append((m.start(), i))
    return spans


# Textes non affichés : macros de traduction (déjà couverts), journaux, erreurs internes.
SKIP_MACROS = (r"tr|trc|trn|tr_noop|trc_noop|tr_later|trn_later|log::\w+|debug|info|warn|error|trace|"
               r"eprintln|println|eprint|print|panic|unreachable|assert\w*|include_\w+|env|option_env|concat|matches|cfg")


def slint_callback_arg(before):
    """La chaîne est-elle un argument d'appel de callback (`Ui.action("code")`) ? Ce sont
    des codes internes, jamais affichés tels quels."""
    depth = 0
    for i in range(len(before) - 1, -1, -1):
        c = before[i]
        if c == ")":
            depth += 1
        elif c == "(":
            if depth == 0:
                return re.search(r"[A-Za-z][\w-]*\.[\w-]+\s*$", before[:i]) is not None
            depth -= 1
    return False


# Mots en minuscules utilisés comme valeurs internes dans les .slint (identifiants, codes).
SLINT_CODES = set()


def allowed(allow, rel, t):
    return t in allow or f"{rel}:{t}" in allow or f"{rel}:*" in allow


def lint():
    """Textes visibles restés hors du système de traduction (Slint et Rust). Les exceptions
    voulues (noms de marque, protocoles, documentation interne) sont dans lint-allow.txt :
    « texte », « chemin:texte » ou « chemin:* »."""
    allow = set()
    if os.path.exists(ALLOW):
        allow = {l.rstrip("\n") for l in open(ALLOW, encoding="utf-8") if l.strip() and not l.startswith("#")}
    problems = []
    for f in sources():
        rel = os.path.relpath(f, ROOT)
        text = open(f, encoding="utf-8").read()
        if f.endswith(".slint"):
            for n, line in enumerate(strip_line_comments(text).split("\n"), 1):
                if re.match(r"\s*import\b", line) or " from \"" in line or "@image-url(" in line:
                    continue
                for m in re.finditer(SLINT_STR, line):
                    t = m.group(1)
                    before = line[:m.start()]
                    if re.search(r"@tr\([^)]*$", before) or before.rstrip().endswith(("|", "=>")):
                        continue
                    if re.search(r"(==|!=)\s*$", before) or re.match(r"\s*(==|!=)", line[m.end():]):
                        continue
                    if re.fullmatch(r"\s*%\s*", t) and "+" in before[-40:] and not allowed(allow, rel, t):
                        problems.append(f"{rel}:{n}: « {t} » collé à un nombre (utiliser @tr(\"{{}} %\", x))")
                        continue
                    if re.fullmatch(r"[0-9]+ %", t) and not allowed(allow, rel, t):
                        problems.append(f"{rel}:{n}: « {t} » (utiliser @tr(\"{{}} %\", x))")
                        continue
                    if not re.search(r"[A-Za-zÀ-ÿ]", t) or (re.fullmatch(r"[a-z0-9_\-.:]+", t) and (re.search(r"[0-9_\-.:]", t) or t in SLINT_CODES)):
                        continue
                    if re.search(r"(font-family|default-font-family|display|font|key)\s*:\s*$", before):
                        continue
                    if slint_callback_arg(before):
                        continue
                    if not allowed(allow, rel, t):
                        problems.append(f"{rel}:{n}: {t}")
        else:
            cut = re.search(r"#\[cfg\(test\)\]\s*(pub(\(\w+\))?\s+)?mod\s+\w+", text)
            if cut:
                text = text[:cut.start()]
            strings, masked = rust_tokens(text)
            spans = macro_spans(masked, SKIP_MACROS) + call_spans(masked, r"(?:Stopwatch::new|\.lap|\.expect)")
            starts = [0]
            for line in text.split("\n"):
                starts.append(starts[-1] + len(line) + 1)
            for a, b, t in strings:
                if text[a] != '"':
                    continue  # chaînes brutes ou d'octets : SQL, données
                if re.search(r"\} %(?![A-Za-z])", t) and not any(x <= a < y for x, y in spans) and not allowed(allow, rel, t):
                    line_no = next(i for i, st in enumerate(starts) if st > a)
                    problems.append(f"{rel}:{line_no}: « {t} » : pourcentage écrit à la française hors traduction (tr!(\"{{v}} %\"))")
                    continue
                if len(t) < 3 or not FR_WORDS.search(t) or allowed(allow, rel, t):
                    continue
                if any(x <= a < y for x, y in spans):
                    continue
                n = next(i for i, st in enumerate(starts) if st > a)
                line = text[starts[n - 1]:starts[n] - 1].strip()
                if line.startswith("#["):
                    continue
                if re.match(r"\s*(=>|\|)", text[b:b + 4]):  # motif de match
                    continue
                problems.append(f"{rel}:{n}: {t}")
    for p in problems:
        print(p)
    print(f"{len(problems)} texte(s) visible(s) hors traduction", file=sys.stderr)
    return 1 if problems else 0


def add_language(code, name, plural, thousands, decimal):
    LANGUAGES[code] = (name, plural, thousands, decimal)
    os.makedirs(os.path.join(LOCALES, code), exist_ok=True)
    extract()
    print(f"{code} créé : traduire les msgstr de locales/{code}/*.po")


if __name__ == "__main__":
    cmd = sys.argv[1] if len(sys.argv) > 1 else "help"
    if cmd == "extract":
        extract()
    elif cmd == "status":
        status()
    elif cmd == "lint":
        sys.exit(lint())
    elif cmd == "add-language":
        add_language(*sys.argv[2:7])
    else:
        print(__doc__)

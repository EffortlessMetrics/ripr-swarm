import re, random
OPS = [
 (r"<=", "<"), (r"(?<![<>=!-])<(?![<=])", "<="), (r">=", ">"), (r"(?<![<>=!-])>(?![>=])", ">="),
 (r"==", "!="), (r"!=", "=="), (r"&&", "||"), (r"\|\|", "&&"),
 (r" \+ ", " - "), (r" - ", " + "), (r" \* ", " / "), (r" / ", " * "), (r" % ", " / "),
 (r"\btrue\b", "false"), (r"\bfalse\b", "true"),
 (r"\.min\(", ".max("), (r"\.max\(", ".min("), (r"checked_", "wrapping_"),
 (r"(?<![\w.\[])(\d+)(?![\w.\]])", "NUM+1"), (r"(?<![\w.\[])([1-9]\d*)(?![\w.\]])", "NUM-1"),
 (r"\bSome\(b'\+'\)", "None"),
]
def gen(src, seed=7, cap=45):
    """Single-site mechanical mutants of non-comment, non-attribute source lines. Returns (old_line_unique_context, new) pairs as full-source replacements."""
    lines = src.split("\n"); cands = []
    for ln, line in enumerate(lines):
        s = line.strip()
        if not s or s.startswith("//") or s.startswith("#[") or s.startswith("pub struct") or s.startswith("pub enum") or s.startswith("use ") or s.startswith("const ") or "fn " in s and s.endswith("{") and "->" not in s:
            continue
        for pat, rep in OPS:
            for m in re.finditer(pat, line):
                if rep.startswith("NUM"):
                    n = int(m.group(1)); n2 = n + 1 if rep == "NUM+1" else n - 1
                    if n2 < 0: continue
                    new = line[:m.start()] + str(n2) + line[m.end():]
                else:
                    new = line[:m.start()] + re.sub(pat, rep, m.group(0)) + line[m.end():]
                if new != line: cands.append((ln, new))
    seen = set(); uniq = []
    for ln, new in cands:
        if (ln, new) in seen: continue
        seen.add((ln, new)); uniq.append((ln, new))
    random.Random(seed).shuffle(uniq)
    return [(ln, new) for ln, new in sorted(uniq[:cap])]
def apply(src, ln, new):
    lines = src.split("\n"); lines[ln] = new; return "\n".join(lines)

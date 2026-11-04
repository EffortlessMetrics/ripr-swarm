"""Per-config scoreboard from cohort receipts.  usage: scoreboard.py OUT_PREFIX receipts.jsonl [...]
Groups runs by (ripr version, model, turn budget, config) and reports each group against the
`none` control of the same (model, turn budget, task set). Re-run on any ripr binary: set
COHORT_RIPR_BIN and re-run sweep.py, then this script."""
import sys, json, math, statistics as st, collections

def wilson(k, n, z=1.96):
    if n == 0: return (0.0, 0.0)
    p = k / n; d = 1 + z * z / n
    c = p + z * z / (2 * n); r = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n))
    return (max(0.0, (c - r) / d), min(1.0, (c + r) / d))

def ver(r):
    v = r.get("ripr_version") or "-"
    return v.split(" ")[1] + ("/" + v.split("(")[1][:7] if "(" in v else "") if v != "-" else "-"

def load(paths):
    rows = []
    for p in paths:
        for l in open(p):
            r = json.loads(l)
            if "error" in r: continue
            r.setdefault("arm", r.get("arm")); rows.append(r)
    return rows

def cfgname(r):  # legacy receipts used A0/A1/A2 arm names
    return {"A0-none": "none", "A1-diagnostics": "check-mandatory", "A2-packet-loop": "brief-mandatory"}.get(r["arm"], r["arm"])

def main(out, paths):
    rows = load(paths)
    groups = collections.defaultdict(list)
    for r in rows: groups[(ver(r), r["model"], r.get("max_turns", 40), cfgname(r))].append(r)
    ctrl = {}
    for (v, m, t, c), rs in groups.items():
        if c == "none": ctrl[(m, t)] = rs
    table = []
    for (v, m, t, c), rs in sorted(groups.items()):
        n = len(rs); fixed = sum(1 for r in rs if r.get("valid") and r["killed"] == r["mutants"])
        invalid = sum(1 for r in rs if not r.get("valid"))
        esc = [(r["mutants"] - r["killed"]) / r["mutants"] if r.get("valid") else 1.0 for r in rs]
        lo, hi = wilson(fixed, n)
        row = dict(ripr=v, model=m, max_turns=t, config=c, runs=n, fix_rate=round(fixed / n, 3), fix_ci95=[round(lo, 3), round(hi, 3)],
                   invalid_runs=invalid, escape_rate=round(st.mean(esc), 4), escaped_mutants=sum(r["mutants"] - r["killed"] for r in rs if r.get("valid")),
                   tokens=round(st.mean(r.get("tokens") or 0 for r in rs)), cost_usd=round(st.mean(r.get("cost_usd") or 0 for r in rs), 4),
                   wall_s=round(st.mean(r["agent_wall_s"] for r in rs), 1), turns=round(st.mean(r.get("turns") or 0 for r in rs), 1),
                   ripr_calls=round(st.mean(r.get("ripr_calls") or 0 for r in rs), 2))
        base = ctrl.get((m, t))
        if base and c != "none":
            bf = sum(1 for r in base if r.get("valid") and r["killed"] == r["mutants"]) / len(base)
            row["fix_rate_vs_none"] = round(row["fix_rate"] - bf, 3)
            row["tokens_vs_none_x"] = round(row["tokens"] / max(1, st.mean(r.get("tokens") or 0 for r in base)), 2)
        table.append(row)
    json.dump(table, open(out + ".json", "w"), indent=1)
    with open(out + ".md", "w") as f:
        f.write("| ripr | model | turn cap | config | runs | fix rate (95% CI) | vs none | escaped / run | tokens | x none | wall s | ripr calls |\n|---|---|---|---|---|---|---|---|---|---|---|---|\n")
        for r in table:
            f.write(
                    f"| {r['ripr']} | {r['model']} | {r['max_turns']} | {r['config']} | {r['runs']} | {r['fix_rate']:.0%} ({r['fix_ci95'][0]:.0%}-{r['fix_ci95'][1]:.0%}) | "
                    f"{(format(r['fix_rate_vs_none'], '+.0%') if 'fix_rate_vs_none' in r else '-')} | {r['escape_rate']:.1%} | {r['tokens']:,} | "
                    f"{r.get('tokens_vs_none_x', '-')} | {r['wall_s']} | {r['ripr_calls']} |\n")
    print(open(out + ".md").read())

if __name__ == "__main__": main(sys.argv[1], sys.argv[2:])

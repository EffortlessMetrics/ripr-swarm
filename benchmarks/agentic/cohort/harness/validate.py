import sys, os, shutil
from lib import *
W = "/tmp/cohort-validate"; os.makedirs(W, exist_ok=True)
for id, t in TASKS.items():
    d = f"{W}/{id}"; build_repo(t, d)
    tgt = f"{W}/target-{id}"
    weak = score(t, d, tgt)
    write(f"{d}/tests/behavior.rs", t["ref_tests"])
    ref = score(t, d, tgt)
    print(id, "weak:", {k: weak[k] for k in ("valid","killed","survived")}, "| ref:", {k: ref[k] for k in ("valid","killed","survived","error","mutant_compile_errors") if k in ref}, flush=True)

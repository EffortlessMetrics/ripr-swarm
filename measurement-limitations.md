# Completed-workload comparison and limits

Both series use unchanged frozen debug binaries, the same 147670-byte source and manifest/lock/diff hashes, Rust1.95.0, Rayon4, initially absent caches, mode ready and a30-second external deadline per run. No compiler work overlapped either series. All12 runs completed exit0, produced byte-identical nonempty-finding JSON and left no owned processes. Source input was also compiled independently before timing.

Original series (top-level benchmark.json): baseline wall samples5.1551/4.6893/4.5777s, candidate4.7458/4.4092/4.6616s; median4.6893→4.6616s (0.6%). Its RSS values are unsuitable for comparison: whole-binary read_bytes hashing between launches inflated the Python parent's resident set, and Linux wait4 child maximum RSS includes the inherited pre-exec floor. The original raw outputs/receipts are retained unchanged.

Corrected series (corrected-benchmark/benchmark.json): fresh harness process, streaming1MiB binary hashing, unchanged candidate and input identities. Baseline wall4.7422/5.3075/4.3950s; candidate4.0189/4.7469/4.0882s. Median4.7422→4.0882s (13.8% lower). Root wait4 CPU medians4.7698→4.1140s; peak root RSS68204→68396KiB. The corrected RSS values no longer show the inflated parent floor; no memory reduction is claimed.

Only three pairs per series, debug builds, one synthetic traversal-heavy input and overlapping ranges. The differing series medians leave the size/repeatability of a performance benefit uncertain. Do not pool series or choose only favorable samples. This is a bounded optimization prototype for review, not a general speed guarantee, absence-of-regression result, hosted-timeout explanation, full-repository qualification or Ready clearance. Original #8201 censored full-repo results remain separate.

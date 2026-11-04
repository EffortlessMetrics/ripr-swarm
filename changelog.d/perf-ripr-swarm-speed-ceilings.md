<!-- section: Changed -->
- The DX scoreboard now carries runner-independent ratchet ceilings on the `ripr-swarm` and `ripr-swarm-2026-10` rows (warm check time and memory; cold pilot time and memory on `ripr-swarm-2026-10`). With `--gate`, a sample above its ceiling, or a selected ceilinged row that produced no value for that metric, fails the run instead of reading as below target (#12947).

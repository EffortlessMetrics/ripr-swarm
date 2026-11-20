<!-- section: Changed -->
- A diff check answers each changed function's transitive and past-the-bound
  reach witness once instead of once per probe. On a warm
  `ripr check --base HEAD~1` over ripr-swarm (656 probes), the witness step
  drops from about 8-9 s to about 1 s, with byte-identical JSON output
  (#13754).

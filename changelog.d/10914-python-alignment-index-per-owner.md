<!-- section: Changed -->
- Python: the sink-alignment index (the test-name lookup plus the owner's import aliases across every test) is built once per changed owner per run instead of once per changed line. Measured warm on 4 cores against main 387f96eb, with byte-identical output: pandas ecdb4258 went from 4.0–4.4 s to 3.2–3.5 s; Django 5c13b5a is unchanged at 2.5–2.9 s (#10914).

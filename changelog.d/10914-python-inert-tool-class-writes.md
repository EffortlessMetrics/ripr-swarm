<!-- section: Changed -->
- Python: the per-file fixture facts keep only the inert tool class names (`CliRunner`) a fixture can return, instead of every identifier in the file, so warm checks no longer decode them. Local warm runs against main a97f1818: pandas 612→497 MiB and 2.4–2.6→2.2–2.3 s; Django 430→357 MiB and 2.2–2.3→2.0 s; findings byte-identical (#10914).

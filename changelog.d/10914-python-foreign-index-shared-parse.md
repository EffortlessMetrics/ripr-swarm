<!-- section: Changed -->
- Python: the workspace foreign-write index now reads each file's assigned attributes from the source-fact parse instead of parsing every workspace file a second time. Measured on 4d54f6d, warm, 4 cores, three interleaved runs each, with byte-identical output: pandas ecdb4258 went from 12.3–13.4 s to 11.8–12.4 s, and Django 5c13b5a from 7.7–8.2 s to 6.7–7.3 s (#10914).

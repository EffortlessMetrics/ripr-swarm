<!-- section: Changed -->
- Python diffs that change many files no longer rebuild the whole workspace's write sets once per changed file. The module-value index counts the sources behind each write once and subtracts the excluded file, so a 167-file Django diff falls from 92.6 s to 15.7 s with byte-identical findings (#10857).

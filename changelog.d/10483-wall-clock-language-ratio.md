<!-- section: Internal / CI -->
- TypeScript, Python and Perl scale tests (line index, identifier writes, deleted-line runs, fact extraction, Perl keyed-index projection) compare the same work at two sizes through `testing::scale::assert_growth_within` instead of asserting an absolute wall-clock bound, so required runs stop failing on runner speed (#10483).

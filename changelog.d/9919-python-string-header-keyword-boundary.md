<!-- section: Fixed -->
- Python: a block header that opens a multi-line string and is written with a tab, no space or a parenthesized condition (`for\ts in`, `for(s)in`, `while(...)`, `if(...)`) is recognized as a header, so a guarded-arm walk no longer attaches an earlier sibling `if` to its body and wrongly excludes an executed return (#9919).

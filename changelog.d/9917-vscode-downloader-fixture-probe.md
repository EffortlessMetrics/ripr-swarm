<!-- section: Fixed -->
- The VS Code downloader fallback test now packages a small shell script as its fixture server instead of a renamed copy of `/bin/echo`, which multicall coreutils builds reject with `unknown program 'ripr-fixture'` (#9917).

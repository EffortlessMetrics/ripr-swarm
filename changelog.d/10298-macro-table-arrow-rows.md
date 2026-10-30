<!-- section: Changed -->
- A local `macro_rules!` test table whose rows are written `name => (..)` now expands one test per row, so the `=>` row layout reads like the same table written with `,`, `;` or `:`; a spaced `= >` or a longer punct ending in `=>` still indexes nothing (#10298).

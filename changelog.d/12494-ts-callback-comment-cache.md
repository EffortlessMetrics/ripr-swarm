<!-- section: Fixed -->
- A comment after the closing `;` of a TypeScript table-test callback (`expect(f(x)).toBe(y); // note`) no longer demotes the row to `weakly_exposed`, and cached TypeScript table results are refreshed after the callback-comment fixes (cache schema 1.141). A new end-to-end fixture pins line, block and trailing comments (RIPR-SPEC-0243, #12494).

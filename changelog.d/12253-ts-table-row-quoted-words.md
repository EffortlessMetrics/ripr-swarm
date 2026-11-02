<!-- section: Fixed -->
- A TypeScript `test.each` or `for...of` table whose title or string holds an ordinary word such as `with` or `this` now resolves its rows instead of keeping the dynamic-assertion refusal; a real `eval`, `with`, `Function`, `arguments` or `this` in code is still refused (RIPR-SPEC-0243, #12253).

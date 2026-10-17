<!-- section: Changed -->
- JavaScript-only repositories and diffs are no longer described as
  TypeScript. `ripr pilot` builds the documented `javascript` language route
  (`language: "javascript"`, same JSON keys) for JS-family files, and the
  `language_adapter_unavailable` and `language_scope_unsupported` limitations
  name JavaScript files and the TypeScript/JavaScript adapter when JS files
  are present. TypeScript-only and other-language texts are unchanged (#4908).

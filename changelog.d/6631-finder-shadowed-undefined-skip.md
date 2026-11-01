<!-- section: Fixed -->
- TypeScript preview: a test whose `skip`/`todo` option is a shadowed `undefined` stays withheld without a partial-extraction gap. The finder now uses the same scope-aware option fence as the extractor, so that inactive registration is not reported as a dropped `test`/`it` call (#6631).

<!-- section: Fixed -->
- TypeScript: a JSX render `<Component .../>` in a `.tsx`/`.jsx` test now counts as an imported owner call of that component, so a test that renders the changed component is no longer reported as not reaching it or told to add a direct owner call. A render never credits oracle strength by itself (#15295).

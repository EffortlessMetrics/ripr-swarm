<!-- section: Fixed -->
- TypeScript class receivers: a body-local `class Url {}` now withholds the constructor credit even when the test also imports the owner's class under that name, as a named import, a named-import alias or a default import (#14083, follow-up to #13726). The import binding used to put the shadowed name back among the constructor names.

<!-- section: Fixed -->
- Analysis: a `use` leaf spelled as a raw identifier (`use crate::b::r#render;`) now binds the plain name, so a bare `render(..)` it binds is read as that import instead of a glob of another module. A change in the other module's `render` is no longer reported `exposed` through such a test, and a change in the imported one keeps its credit (#14087).

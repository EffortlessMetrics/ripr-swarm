<!-- section: Fixed -->
- The extension publish workflow waits for the generation-keyed server manifest (`ripr-server-manifest-vX.Y.Z.json`) under the RC tag, matching the editor's RC manifest placement, instead of a manifest named after the full RC version that is never published (#3798, found by the #3807 trust audit).

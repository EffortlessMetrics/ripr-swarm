<!-- section: Fixed -->
- `ripr pr-review front-panel` Markdown names the input whose warning promoted
  the panel to `missing_required_input` on the `Missing input:` line when that
  warning came from a read failure on the assistant proof, instead of whichever
  warning happened to be first; a warning synthesized from a first-action
  fallback no longer outranks a real unusable input such as an unreadable
  receipt (#14628).

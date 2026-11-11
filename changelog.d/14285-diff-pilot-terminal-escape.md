<!-- section: Security -->
- The human `ripr diff` report and the `ripr pilot` terminal summary now spell terminal control and bidi characters from repository file names, seam ids and evidence lines as `\u{XX}`, as `ripr check` already did, so a hostile file name cannot retitle the window, clear the screen or reorder the text. JSON output is unchanged (#14285).

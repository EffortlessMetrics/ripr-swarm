<!-- section: Internal / CI -->
- The Windows hard-kill helper test now stops the helper's already-recorded survivors when its own setup fails (descendant marker never written, primary marker unreadable, descendant not alive), so a failed setup no longer leaves processes holding the harness's output pipe for up to 40 s (#12223).

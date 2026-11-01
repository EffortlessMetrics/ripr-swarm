<!-- section: Added -->
- The DX scoreboard now records the peak memory of `ripr lsp --stdio` after an edited save (`speed.editor_refresh_peak_rss_mb`, Linux) and gates it with a 1024 MB target, so editor-server residency has a trend and a regression bar like `ripr check` does (#11586).

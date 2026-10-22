<!-- section: Fixed -->
- LSP work-done progress no longer drops the first stage reports ("loading input") when the client answers `window/workDoneProgress/create` after analysis has already started. Stages that arrive before `Begin` are held and sent in order right after it; nothing precedes `Begin` and a token ended before `Begin` stays silent (#9362).

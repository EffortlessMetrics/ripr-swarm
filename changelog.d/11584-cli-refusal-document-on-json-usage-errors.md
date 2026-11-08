<!-- section: Changed -->
- JSON refusals: `ripr check` refusal documents carry `status: "refused"` and `error {code, message, next}`, and argv usage errors and `context` selector misses under an explicit JSON request write a minimal `cli_refusal` document (#11584, #6834).

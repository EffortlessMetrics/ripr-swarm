# Fixture: recording_double_observer_refusals

Spec: RIPR-SPEC-0240

## Given

Seven same-file recording-double shapes, with unique owner leaf names so owner
uniqueness cannot mask another guard. The owned-clone test calls a raw-named,
by-value user clone that returns a fixed vector. The raw-binding and Unicode NFC
binding tests shadow a slice with an integer whose user iterator exits before
the asserted field is observed. A raw-named local Recorder shadows a trusted
root double and writes a constant instead of the sent argument. A const-pattern
let-else exits before the changed call. A borrowed trait `push` on the standard
vector intercepts the writer and exits before retaining its argument.
An assertion's formatted diagnostic exits only when the recorded value is wrong,
before the test harness can report the failed comparison.

Separate native controls show clone and receiver-alias originals and wrong
payloads both complete passing harnesses. Raw/Unicode binding, let-else and
borrowed-push controls exit before a completed harness; they are execution fault
witnesses, not passing test qualification. The isolated admissible exact-line
positive remains in the public verdict corpus and focused API suite.
The diagnostic-exit original completes a passing harness; its wrong-input
variant exits without a completed harness and is also an execution fault witness.

## When

The public analyzer reads the seven formatting rewrites with all input files.

## Then

Fourteen call-deletion/side-effect findings remain static_unknown. The independent
honesty-corpus count and must_not_promote assertions reject empty output and an
incorrect exposed re-bless. The frozen pre-repair analyzer is a discriminating
control for the original five target owners. The frozen a0146be7 analyzer
falsely exposes the borrowed-push pair while retaining the original ten refusals.
The frozen 833bc135 analyzer falsely exposes the assertion-diagnostic pair while
retaining the original twelve refusal objects unchanged.

## Must Not

- Borrow a user clone as standard Vec observation, including raw spellings.
- Carry stale slice provenance through raw or Unicode-normalized binding aliases.
- Confuse early process exit with a completed test harness.
- Grant fixture-builder, count-only or concrete collection credit.
- Assume a method named `push` necessarily reaches standard vector retention.
- Ignore diagnostic arguments that execute when the comparison fails.

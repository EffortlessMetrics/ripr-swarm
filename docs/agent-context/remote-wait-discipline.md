# Remote wait discipline

Use this when GitHub, a review provider, or another owned remote system controls
the next transition for one or more delivery lanes. It is a decision protocol,
not a background monitor.

## One wake, one snapshot

At a wake boundary, read the relevant native objects once as one batch where the
API permits it. For each wait retain:

- repository and PR number;
- exact candidate head and, when relevant, run/job identity;
- observed state and timestamp;
- the event that can change the next decision;
- the earliest sensible recheck if no event reaches the current session.

Do not query the same unchanged PR through several overlapping commands merely
to create activity. A check list, run detail, review-thread inventory and merge
state are different authorities; read each only when that distinction affects
the decision.

## Select work before selecting a timer

After the snapshot:

1. Handle a material event already present: failure, cancellation, changed head,
   new finding, completed required proof, or merged/closed disposition.
2. Otherwise select another already-authorized independent claim, review,
   repair, or bounded evidence task. Preserve one writer per candidate.
3. If no useful work remains, retain `GOAL_IN_FLIGHT` with the exact waits and
   accountable owner. A bounded worker may yield to that owner or a supported,
   actually arranged continuation. The owner uses supported spaced waits or events
   within the task window, then inspects the result. On interruption or exhaustion,
   leave an exact durable handoff; never claim unarranged background monitoring or
   create a resident polling worker.

Unrelated work is not made relevant by spare capacity. Productive waiting uses
the accepted goal and current backlog; it does not manufacture adjacent scope.

## Bounded rechecks

For hosted stages whose observed duration is normally measured in tens of
minutes, a status-only recheck less than five minutes after the last unchanged
snapshot is normally too early. After one unchanged recheck, widen rather than
shorten the interval. A known approximate terminal boundary can justify one
closer read; it does not justify repeated 30/45/60/90-second loops.

This is not an ETA model. Elapsed time never proves completion, and a historical
median does not establish the current run. A material event already delivered
to the session resumes immediately; the spacing rule never delays evidence that
is already present.

Do not use `gh run watch`, a tight shell loop, repeated timers, or a subagent
whose only job is polling. Do not hold a model session open solely to preserve a
provider cache. The older cache-TTL watcher learning explains one historical
cost tradeoff; current delivery law prefers useful work or an honest yield over
an idle polling loop.

## Failure, retry and uncertain writes

Classify source, test/oracle, instrument and infrastructure failures using the
actual head and applicable evidence; keep an unknown cause unknown. A deterministic
failure calls for investigation or repair, not repeated identical runs. A bounded
retry is appropriate for a demonstrated transient when it can change the decision.
A denied operation blocks only dependent work: retain the tool, exact action,
target and stated reason, and continue independent authorized work. A denied read
does not by itself establish a denial of a distinct write. Assess each action's
authorization, dependencies and actual holds separately; do not use a distinct
write to obtain data from a denied read or otherwise route around that denial.
Do not retry an unchanged denial
or seek broader permissions as a generic remedy. If a denied operation is known
not to have executed, retry it at most once after material new evidence changes
the basis for that operation, subject to the host's restrictions. If a write's
outcome is uncertain,
read back the native object before deciding whether any retry is safe; never assume
it failed or duplicate a merge, label request, reply or publication blindly.

## Resume contract

A resumed owner rehydrates the exact PR/head/run on the relevant event or supported
recheck and resumes at the earliest missing judgment. Re-read native state; do not trust the old timer,
summary, or predicted completion.

A wait may resolve to:

- `REPAIR_REQUIRED` for a valid failure or finding;
- `REVIEW_INCOMPLETE` for missing current substantive review;
- `WAITING_REQUIRED_CHECKS` or `WAITING_EXTERNAL_REVIEW` when remote ownership
  remains;
- `PR_IN_FLIGHT` / `GOAL_IN_FLIGHT` when the lane is honestly yielded;
- the ordinary finish path only after the separately required review,
  qualification and final integration judgments are current.

## Discriminating scenarios

### Long run plus ready work

A required test stage has historically taken about 29 minutes and is still
running. Another independent accepted claim is ready. Take one current snapshot,
record the run/head and terminal event, then advance the ready claim. Scheduling
a one-minute timer is the wrong result.

### All lanes remote-owned

Every accepted lane is waiting on CI or external review and no authorized local
work remains. Retain `GOAL_IN_FLIGHT` with each exact wake event and accountable
owner. A bounded worker may return to the owner, who continues within the actual
task window or leaves an honest durable handoff on interruption. Starting an idle
polling agent or claiming an unarranged watcher is the wrong result.

### Material event

A check has failed, the head changed, or a review finding arrived. Inspect it
now. The bounded-recheck rule cannot be used to postpone an event already
observed.

------------------------- MODULE RepairAttemptFinish -------------------------
EXTENDS Naturals, FiniteSets

CONSTANTS IgnoreCurrentness, IgnoreCageVerdict, BypassAwaitingGuard

Workers == 1..2
Verdicts == {"compliant", "violated", "incomparable"}
States == {"awaiting_edit", "ready_to_finish", "failed", "incomparable", "stale"}

VARIABLES phase, sampleCurrent, sampleVerdict, state, finishCount,
          winnerCurrent, winnerVerdict

vars == <<phase, sampleCurrent, sampleVerdict, state, finishCount,
          winnerCurrent, winnerVerdict>>

\* Abstracts repair_attempt_state_for_finish, not receipt currentness.
FinishState(current, verdict) ==
    IF ~current /\ ~IgnoreCurrentness THEN "stale"
    ELSE IF IgnoreCageVerdict THEN "ready_to_finish"
    ELSE CASE verdict = "compliant" -> "ready_to_finish"
           [] verdict = "violated" -> "failed"
           [] OTHER -> "incomparable"

Init ==
    /\ phase = [w \in Workers |-> "idle"]
    /\ sampleCurrent = [w \in Workers |-> TRUE]
    /\ sampleVerdict = [w \in Workers |-> "compliant"]
    /\ state = "awaiting_edit"
    /\ finishCount = 0
    /\ winnerCurrent = TRUE
    /\ winnerVerdict = "compliant"

\* Each finisher captures its own after-phase observations. The production
\* producer forces a non-current verdict to incomparable; allowing all six
\* pairs here overapproximates that producer and tests the classifier itself.
Capture(w) ==
    /\ phase[w] = "idle"
    /\ state = "awaiting_edit"
    /\ \E current \in BOOLEAN, verdict \in Verdicts:
        /\ phase' = [phase EXCEPT ![w] = "captured"]
        /\ sampleCurrent' = [sampleCurrent EXCEPT ![w] = current]
        /\ sampleVerdict' = [sampleVerdict EXCEPT ![w] = verdict]
    /\ UNCHANGED <<state, finishCount, winnerCurrent, winnerVerdict>>

\* The locked manifest recheck and write are ONE abstract atomic transition.
\* This does not establish that the Rust/filesystem implementation is atomic.
Commit(w) ==
    /\ phase[w] = "captured"
    /\ (state = "awaiting_edit" \/ BypassAwaitingGuard)
    /\ state' = FinishState(sampleCurrent[w], sampleVerdict[w])
    /\ finishCount' = finishCount + 1
    /\ winnerCurrent' = sampleCurrent[w]
    /\ winnerVerdict' = sampleVerdict[w]
    /\ phase' = [phase EXCEPT ![w] = "done"]
    /\ UNCHANGED <<sampleCurrent, sampleVerdict>>

Refuse(w) ==
    /\ phase[w] = "captured"
    /\ state # "awaiting_edit"
    /\ ~BypassAwaitingGuard
    /\ phase' = [phase EXCEPT ![w] = "done"]
    /\ UNCHANGED <<sampleCurrent, sampleVerdict, state, finishCount,
                   winnerCurrent, winnerVerdict>>

\* Intentional terminal stutter, not a global -deadlock suppression.
Quiesce ==
    /\ state # "awaiting_edit"
    /\ \A w \in Workers: phase[w] # "captured"
    /\ UNCHANGED vars

Next == (\E w \in Workers: Capture(w) \/ Commit(w) \/ Refuse(w)) \/ Quiesce

TypeOK ==
    /\ phase \in [Workers -> {"idle", "captured", "done"}]
    /\ sampleCurrent \in [Workers -> BOOLEAN]
    /\ sampleVerdict \in [Workers -> Verdicts]
    /\ state \in States
    /\ finishCount \in 0..2
    /\ winnerCurrent \in BOOLEAN
    /\ winnerVerdict \in Verdicts

FinishRequiresCurrentSnapshot == state = "ready_to_finish" => winnerCurrent
FinishRequiresCompliantCage == state = "ready_to_finish" => winnerVerdict = "compliant"
AtMostOneFinish == finishCount <= 1
=============================================================================

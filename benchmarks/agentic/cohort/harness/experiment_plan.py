"""Freeze the complete trial population before collecting cohort receipts.

Version 1 orders blocks by seeded SHA-256, then rotates a seeded arm permutation
within each task/model across repetitions. Identity hashes pin declared inputs;
they do not attest that tasks or families are independently authored. Each oracle
is the hash of its ordered mutation-digest vector, matching scorer mutant IDs.
"""
import json
import math
from pathlib import Path
import re

from measurements import digest

MAX_TRIALS = 100_000  # Hard expansion bound, checked before allocating schedule rows.
STRATA = frozenset({"routine", "no_op", "negative_control", "challenging"})
PARTITIONS = frozenset({"development", "holdout"})
LEGACY_ARMS = frozenset({"A0-none", "A1-diagnostics", "A2-packet-loop"})
DESIGN_KEYS = frozenset({"design_schema_version", "experiment_id", "seed",
                       "repetitions", "max_turns", "trial_timeout_s",
                       "protocol_sha256", "evaluator_sha256", "runner",
                       "control_arm", "models", "arms", "tasks"})
RUNNER_KEYS = frozenset({"id", "environment_sha256"})
MODEL_KEYS = frozenset({"id", "provider", "revision", "settings_sha256"})
ARM_KEYS = frozenset({"id", "configuration_sha256", "ripr_revision", "ripr_binary_sha256"})
TASK_KEYS = frozenset({"id", "family", "stratum", "partition", "task_sha256",
                     "oracle_sha256", "mutants", "mutant_sha256s"})
PLAN_KEYS = frozenset({"plan_schema_version", "design", "design_sha256", "schedule", "plan_sha256"})
ROW_KEYS = frozenset({"trial_id", "block_id", "task", "model", "arm", "rep", "position"})


def _object(value, keys, at):
    if type(value) is not dict or value.keys() != keys:
        raise ValueError(f"{at}: expected exactly {', '.join(sorted(keys))}")
    return value


def _text(value, at, *, identifier=False):
    if type(value) is not str or not value.strip():
        raise ValueError(f"{at}: expected nonblank string")
    if identifier and any(c.isspace() or not c.isprintable() for c in value):
        raise ValueError(f"{at}: identifiers cannot contain whitespace or controls")
    return value


def _integer(value, at, minimum=1):
    if type(value) is not int or value < minimum:
        raise ValueError(f"{at}: expected integer >= {minimum}")
    return value


def _sha(value, at):
    if type(value) is not str or re.fullmatch(r"[0-9a-fA-F]{64}", value) is None:
        raise ValueError(f"{at}: expected 64 hex characters")
    return value.lower()


def _records(value, keys, at, minimum=1):
    if type(value) is not list or len(value) < minimum:
        raise ValueError(f"{at}: expected list with at least {minimum} records")
    seen = set()
    for record in value:
        _object(record, keys, at)
        name = _text(record["id"], f"{at}.id", identifier=True)
        if name in seen:
            raise ValueError(f"{at}: duplicate id {name}")
        seen.add(name)
    return sorted(value, key=lambda record: record["id"])


def _design(raw):
    _object(raw, DESIGN_KEYS, "design")
    if _integer(raw["design_schema_version"], "design_schema_version") != 1:
        raise ValueError("unsupported design_schema_version")
    result = dict(design_schema_version=1,
                  experiment_id=_text(raw["experiment_id"], "experiment_id", identifier=True),
                  seed=_integer(raw["seed"], "seed", 0),
                  control_arm=_text(raw["control_arm"], "control_arm", identifier=True))
    for key in ("repetitions", "max_turns", "trial_timeout_s"):
        result[key] = _integer(raw[key], key)
    for key in ("protocol_sha256", "evaluator_sha256"):
        result[key] = _sha(raw[key], key)
    runner = _object(raw["runner"], RUNNER_KEYS, "runner")
    result["runner"] = dict(id=_text(runner["id"], "runner.id", identifier=True),
                            environment_sha256=_sha(runner["environment_sha256"], "runner.environment_sha256"))
    result["models"] = [dict(id=row["id"], provider=_text(row["provider"], "model.provider"),
                            revision=_text(row["revision"], "model.revision"),
                            settings_sha256=_sha(row["settings_sha256"], "model.settings_sha256"))
                        for row in _records(raw["models"], MODEL_KEYS, "models")]
    result["arms"] = []
    for row in _records(raw["arms"], ARM_KEYS, "arms", 2):
        if row["id"] in LEGACY_ARMS:
            raise ValueError("arm.id: use canonical arm names, not legacy aliases")
        revision, binary = row["ripr_revision"], row["ripr_binary_sha256"]
        if (revision is None) != (binary is None):
            raise ValueError("arm: ripr_revision and ripr_binary_sha256 must both be set or null")
        if revision is not None:
            if type(revision) is not str or re.fullmatch(r"[0-9a-f]{40}", revision) is None:
                raise ValueError("arm.ripr_revision: expected full lowercase Git SHA")
            binary = _sha(binary, "arm.ripr_binary_sha256")
        result["arms"].append(dict(id=row["id"], ripr_revision=revision, ripr_binary_sha256=binary,
                                   configuration_sha256=_sha(row["configuration_sha256"], "arm.configuration_sha256")))
    if result["control_arm"] not in {row["id"] for row in result["arms"]}:
        raise ValueError("control_arm must name a planned arm")
    result["tasks"] = []
    families, task_hashes = {}, {}
    for row in _records(raw["tasks"], TASK_KEYS, "tasks"):
        family = _text(row["family"], "task.family", identifier=True)
        partition = _text(row["partition"], "task.partition")
        stratum = _text(row["stratum"], "task.stratum")
        if partition not in PARTITIONS or stratum not in STRATA:
            raise ValueError("task: unsupported partition or stratum")
        task_hash = _sha(row["task_sha256"], "task.task_sha256")
        if task_hash in task_hashes:
            raise ValueError("task.task_sha256: duplicate task contents cannot add independent subjects")
        for seen, key, name in ((families, family, "family"), (task_hashes, task_hash, "task_sha256")):
            if key in seen and seen[key] != partition:
                raise ValueError(f"task.{name}: cannot cross development/holdout partitions")
            seen[key] = partition
        count = _integer(row["mutants"], "task.mutants")
        hashes = row["mutant_sha256s"]
        if type(hashes) is not list or len(hashes) != count:
            raise ValueError("task.mutant_sha256s: expected one digest per declared mutant")
        hashes = [_sha(value, "task.mutant_sha256s") for value in hashes]
        if len(set(hashes)) != count:
            raise ValueError("task.mutant_sha256s: duplicate mutation digest")
        oracle = _sha(row["oracle_sha256"], "task.oracle_sha256")
        if oracle != digest(hashes):
            raise ValueError("task.oracle_sha256: must hash the ordered mutation-digest vector")
        result["tasks"].append(dict(id=row["id"], family=family, partition=partition, stratum=stratum,
                                    task_sha256=task_hash, oracle_sha256=oracle,
                                    mutants=count, mutant_sha256s=hashes))
    if set(families.values()) != PARTITIONS:
        raise ValueError("tasks must include development and holdout partitions")
    count = len(result["tasks"]) * len(result["models"]) * len(result["arms"]) * result["repetitions"]
    if count > MAX_TRIALS:
        raise ValueError(f"plan exceeds {MAX_TRIALS} trials")
    return result


def _schedule(design, design_hash):
    blocks = []
    for task in design["tasks"]:
        for model in design["models"]:
            pair = [design_hash, task["id"], model["id"]]
            arms = sorted((arm["id"] for arm in design["arms"]),
                          key=lambda arm: (digest(["arm_order", design["seed"], *pair, arm]), arm))
            for rep in range(1, design["repetitions"] + 1):
                block = digest(["block", *pair, rep])
                shift = (rep - 1) % len(arms)
                rows = [dict(trial_id=digest(["trial", *pair, rep, arm]), block_id=block,
                             task=task["id"], model=model["id"], arm=arm, rep=rep)
                        for arm in arms[shift:] + arms[:shift]]
                blocks.append((digest(["block_order", design["seed"], block]), block, rows))
    return [dict(row, position=position) for position, row in enumerate(
        row for _, _, rows in sorted(blocks) for row in rows)]


def freeze(design: dict) -> dict:
    """Validate a design and deterministically freeze all trials and input pins.

    SHA-256 strings normalize to lowercase; record arrays normalize by id.
    Schedule positions are global, zero based; repetitions are one based.
    """
    canonical = _design(design)
    design_hash = digest(canonical)
    plan = dict(plan_schema_version=1, design=canonical, design_sha256=design_hash,
                schedule=_schedule(canonical, design_hash))
    return dict(plan, plan_sha256=digest(plan))


def validate_plan(plan: dict) -> dict:
    """Recompute the canonical plan; reject edits, ignored fields and loose types."""
    _object(plan, PLAN_KEYS, "plan")
    if _integer(plan["plan_schema_version"], "plan_schema_version") != 1:
        raise ValueError("unsupported plan_schema_version")
    for key in ("design_sha256", "plan_sha256"):
        _sha(plan[key], key)
    expected = freeze(plan["design"])
    if type(plan["schedule"]) is not list or len(plan["schedule"]) != len(expected["schedule"]):
        raise ValueError("schedule must contain every planned trial exactly once")
    for row in plan["schedule"]:
        _object(row, ROW_KEYS, "schedule row")
        for key in ("trial_id", "block_id"):
            _sha(row[key], f"schedule.{key}")
        for key in ("task", "model", "arm"):
            _text(row[key], f"schedule.{key}", identifier=True)
        _integer(row["rep"], "schedule.rep")
        _integer(row["position"], "schedule.position", 0)
    # Canonical JSON comparison additionally distinguishes numeric/boolean values
    # inside otherwise equal Python containers (e.g. 1, 1.0 and True).
    try:
        equal = digest(plan) == digest(expected)
    except (TypeError, ValueError, UnicodeError) as error:
        raise ValueError("plan must contain valid canonical JSON values") from error
    if not equal:
        raise ValueError("plan differs from its frozen design, schedule or digests")
    return expected


def parse_json(text: str) -> dict:
    """Decode one JSON object, rejecting duplicate keys and nonfinite numbers."""
    def object_pairs(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise ValueError(f"duplicate JSON key: {key}")
            result[key] = value
        return result

    def finite_float(value):
        number = float(value)
        if not math.isfinite(number):
            raise ValueError("nonfinite JSON number")
        return number

    def constant(value):
        raise ValueError(f"nonfinite JSON number: {value}")

    value = json.loads(text, object_pairs_hook=object_pairs,
                       parse_float=finite_float, parse_constant=constant)
    if type(value) is not dict:
        raise ValueError("JSON root must be an object")
    return value


def read_json(path) -> dict:
    """Read a UTF-8 JSON object with the same strict decoder as receipt loading."""
    return parse_json(Path(path).read_text(encoding="utf-8"))


def write_plan(path, plan: dict) -> None:
    """Create a validated plan exclusively; an existing file is never replaced."""
    encoded = json.dumps(validate_plan(plan), sort_keys=True, indent=2, allow_nan=False) + "\n"
    with Path(path).open("x", encoding="utf-8") as stream:
        stream.write(encoded)

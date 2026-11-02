/**
 * Shared repair after-phase orchestration for the VS Code client (#4647, #2928).
 *
 * Pure helpers: parse the CLI after-phase `--json` stdout (and typed refusals
 * that may arrive on stdout even when the process exits non-zero), filter live
 * inventory rows, and build the canonical after-phase argv.
 */

import {
  parseAgentAttemptStatus,
  resolveActiveAttempt,
  type ActiveAttemptResolution,
  type AgentAttemptStatus,
  type AttemptInventoryRow,
} from './attemptStatus';

/** Operational states that still accept an after-phase finish. */
export const LIVE_REPAIR_ATTEMPT_STATES: ReadonlySet<string> = new Set([
  'awaiting_edit',
  'prepared',
]);

export interface AfterPhaseRepairFinish {
  attemptId: string;
  agentStatus?: AgentAttemptStatus;
  movementLine?: string;
  summaryLines: string[];
  refusalError?: string;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function nonEmptyString(value: unknown): string | undefined {
  return typeof value === 'string' && value.trim() !== '' ? value : undefined;
}

function stringArray(value: unknown): string[] {
  if (!Array.isArray(value)) {
    return [];
  }
  return value.filter((item): item is string => typeof item === 'string' && item.trim() !== '');
}

export function buildAgentRepairAfterArgs(root: string, attemptId: string): string[] {
  return ['agent', 'repair', '--root', root, '--attempt', attemptId, '--phase', 'after', '--json'];
}

export function liveRepairAttemptRows(
  rows: readonly AttemptInventoryRow[]
): AttemptInventoryRow[] {
  return rows.filter(
    (row) => row.state !== null && LIVE_REPAIR_ATTEMPT_STATES.has(row.state)
  );
}

/**
 * Resolve which live attempt the editor should finish. Uses the same selection
 * law as status (#4643) but only over rows still awaiting the external edit.
 */
export function resolveLiveAttemptForFinish(
  rows: readonly AttemptInventoryRow[],
  options?: { explicitAttemptId?: string; rememberedAttemptId?: string }
): ActiveAttemptResolution {
  const live = liveRepairAttemptRows(rows);
  return resolveActiveAttempt(live, options);
}

function movementLineFromVerify(verify: Record<string, unknown>): string | undefined {
  const movement = verify['movement'];
  if (isRecord(movement)) {
    const summary = nonEmptyString(movement['summary']);
    if (summary) {
      return summary;
    }
  }
  const outcome = verify['outcome'];
  if (isRecord(outcome)) {
    const summary = nonEmptyString(outcome['summary']);
    if (summary) {
      return summary;
    }
  }
  return nonEmptyString(verify['status']);
}

/**
 * Parse stdout from `ripr agent repair --phase after --json`. Accepts the
 * success envelope, typed refusal document, bare verify document, or a lone
 * agent_attempt_status when the attempt was already terminal.
 */
export function parseAfterPhaseRepairFinish(stdout: string): AfterPhaseRepairFinish | undefined {
  const trimmed = stdout.trim();
  if (trimmed === '') {
    return undefined;
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(trimmed) as unknown;
  } catch {
    return undefined;
  }
  if (!isRecord(parsed)) {
    return undefined;
  }
  const kind = nonEmptyString(parsed['kind']);
  const schemaVersion = parsed['schema_version'];

  if (kind === 'repair_after_refusal') {
    const attemptId = nonEmptyString(parsed['attempt_id']) ?? 'unknown';
    const error = nonEmptyString(parsed['error']);
    const narration = stringArray(parsed['narration']);
    const summaryLines = narration.length > 0 ? narration : error ? [error] : [];
    return {
      attemptId,
      refusalError: error,
      summaryLines,
    };
  }

  if (kind === 'repair_after_result' && schemaVersion === '0.1') {
    const verify = isRecord(parsed['verify']) ? parsed['verify'] : undefined;
    const agentRaw = parsed['agent_status'];
    const agentStatus = parseAgentAttemptStatus(agentRaw);
    const attemptId = agentStatus?.attemptId ?? 'unknown';
    const movementLine = verify ? movementLineFromVerify(verify) : undefined;
    const summaryLines: string[] = [];
    if (movementLine) {
      summaryLines.push(movementLine);
    }
    if (agentStatus) {
      summaryLines.push(
        `Attempt ${agentStatus.attemptId} is ${agentStatus.statusClass.replace(/_/g, ' ')} (${agentStatus.currentness}).`
      );
      const next = agentStatus.nextAction?.command ?? agentStatus.nextAction?.step;
      if (next) {
        summaryLines.push(`Next: ${next}`);
      }
    }
    return {
      attemptId,
      agentStatus,
      movementLine,
      summaryLines,
    };
  }

  if (kind === 'agent_attempt_status') {
    const agentStatus = parseAgentAttemptStatus(parsed);
    if (!agentStatus) {
      return undefined;
    }
    return {
      attemptId: agentStatus.attemptId,
      agentStatus,
      summaryLines: [
        `Attempt ${agentStatus.attemptId} is already ${agentStatus.statusClass.replace(/_/g, ' ')} (${agentStatus.currentness}).`,
      ],
    };
  }

  if (schemaVersion === '0.3' && parsed['tool'] === 'ripr') {
    const movementLine = movementLineFromVerify(parsed);
    return {
      attemptId: 'unknown',
      movementLine,
      summaryLines: movementLine ? [movementLine] : [],
    };
  }

  return undefined;
}

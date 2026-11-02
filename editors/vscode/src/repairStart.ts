/**
 * Shared repair before-phase orchestration for the VS Code client (#4645).
 *
 * Pure helpers: parse the CLI before-phase `--json` stdout, detect live
 * attempts for idempotent start, and locate the LSP `agent_repair` handoff
 * target among code actions.
 */

import type { AttemptInventoryRow } from './attemptStatus';

/** Operational states that mean an attempt is still open for the same seam. */
export const LIVE_REPAIR_ATTEMPT_STATES: ReadonlySet<string> = new Set([
  'awaiting_edit',
  'prepared',
]);

/** Inventory disposition that means the CLI would resume this attempt as-is (#4645). */
export const RESUMABLE_REPAIR_ATTEMPT_DISPOSITION = 'resumable';

export interface BeforePhaseRepairStart {
  attemptId: string;
  manifestPath?: string;
  nextCommand?: string;
  seamId?: string;
  relatedTestFile?: string;
  relatedTestLine?: number;
  relatedTestName?: string;
  summaryLines: string[];
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function nonEmptyString(value: unknown): string | undefined {
  return typeof value === 'string' && value.trim() !== '' ? value : undefined;
}

function positiveLine(value: unknown): number | undefined {
  if (typeof value === 'number' && Number.isInteger(value) && value > 0) {
    return value;
  }
  return undefined;
}

function stringArray(value: unknown): string[] {
  if (!Array.isArray(value)) {
    return [];
  }
  return value.filter((item): item is string => typeof item === 'string' && item.trim() !== '');
}

function firstPacket(document: Record<string, unknown>): Record<string, unknown> | undefined {
  const packets = document['packets'];
  if (!Array.isArray(packets) || packets.length === 0) {
    return undefined;
  }
  const first = packets[0];
  return isRecord(first) ? first : undefined;
}

function relatedTestFromPacket(packet: Record<string, unknown>): {
  file?: string;
  line?: number;
  name?: string;
} {
  const imitate = packet['nearest_strong_test_to_imitate'];
  if (isRecord(imitate)) {
    const file = nonEmptyString(imitate['file']);
    const line = positiveLine(imitate['line']);
    const name = nonEmptyString(imitate['name']);
    if (file) {
      return { file, line, name };
    }
  }
  const related = packet['related_existing_tests'];
  if (Array.isArray(related)) {
    for (const entry of related) {
      if (!isRecord(entry)) {
        continue;
      }
      const file = nonEmptyString(entry['file']);
      if (!file) {
        continue;
      }
      return {
        file,
        line: positiveLine(entry['line']),
        name: nonEmptyString(entry['name']),
      };
    }
  }
  const recommended = packet['recommended_test'];
  if (isRecord(recommended)) {
    const file = nonEmptyString(recommended['file']);
    if (file) {
      return {
        file,
        name: nonEmptyString(recommended['name']),
      };
    }
  }
  return {};
}

export function buildAgentRepairBeforeArgs(root: string, seamId: string): string[] {
  return ['agent', 'repair', '--root', root, '--seam-id', seamId, '--phase', 'before', '--json'];
}

export function findLiveRepairAttemptForSeam(
  rows: readonly AttemptInventoryRow[],
  seamId: string
): AttemptInventoryRow | undefined {
  return rows.find(
    (row) =>
      row.seamId === seamId &&
      row.state !== null &&
      LIVE_REPAIR_ATTEMPT_STATES.has(row.state) &&
      row.disposition === RESUMABLE_REPAIR_ATTEMPT_DISPOSITION
  );
}

export function parseBeforePhaseRepairStart(stdout: string): BeforePhaseRepairStart | undefined {
  let parsed: unknown;
  try {
    parsed = JSON.parse(stdout) as unknown;
  } catch {
    return undefined;
  }
  if (!isRecord(parsed)) {
    return undefined;
  }
  const continuation = parsed['repair_attempt'];
  if (!isRecord(continuation)) {
    return undefined;
  }
  const attemptId = nonEmptyString(continuation['attempt_id']);
  if (!attemptId) {
    return undefined;
  }
  const packet = firstPacket(parsed);
  const related = packet ? relatedTestFromPacket(packet) : {};
  const seamId = packet ? nonEmptyString(packet['seam_id']) : undefined;
  const allowed = packet ? stringArray(packet['allowed_edit_surface']) : [];
  const missing = packet
    ? (Array.isArray(packet['missing_discriminators'])
      ? packet['missing_discriminators']
        .map((entry) => (isRecord(entry) ? nonEmptyString(entry['value']) : undefined))
        .filter((value): value is string => value !== undefined)
      : [])
    : [];
  const mustNot = packet ? stringArray(packet['must_not_change']) : [];
  const summaryLines = [
    `Repair attempt ${attemptId} is awaiting an external test edit; the repair is not complete.`,
    seamId ? `Seam: ${seamId}` : undefined,
    missing.length > 0 ? `Missing discriminator: ${missing.join('; ')}` : undefined,
    allowed.length > 0 ? `Allowed edit surface: ${allowed.join(', ')}` : undefined,
    ...mustNot,
    related.file
      ? `Related test: ${related.file}${related.line ? `:${related.line}` : ''}${related.name ? ` (${related.name})` : ''}`
      : undefined,
    nonEmptyString(continuation['next_command'])
      ? `Next after the edit: ${continuation['next_command']}`
      : undefined,
  ].filter((line): line is string => line !== undefined);

  return {
    attemptId,
    manifestPath: nonEmptyString(continuation['manifest_path']),
    nextCommand: nonEmptyString(continuation['next_command']),
    seamId,
    relatedTestFile: related.file,
    relatedTestLine: related.line,
    relatedTestName: related.name,
    summaryLines,
  };
}

function commandPayload(action: unknown): Record<string, unknown> | undefined {
  if (!isRecord(action)) {
    return undefined;
  }
  const embedded = action['command'];
  if (isRecord(embedded) && typeof embedded['command'] === 'string') {
    const args = embedded['arguments'];
    const first = Array.isArray(args) && args.length > 0 ? args[0] : undefined;
    return isRecord(first) ? first : undefined;
  }
  if (typeof embedded === 'string' && typeof action['arguments'] !== 'undefined') {
    const args = action['arguments'];
    const first = Array.isArray(args) && args.length > 0 ? args[0] : undefined;
    return isRecord(first) ? first : undefined;
  }
  return undefined;
}

/** Locate the validated LSP payload for `Start repair: copy repair command`. */
export function extractAgentRepairLoopTarget(
  actions: readonly unknown[]
): Record<string, unknown> | undefined {
  for (const action of actions) {
    if (!isRecord(action)) {
      continue;
    }
    const command = isRecord(action['command']) ? action['command'] : action;
    const commandId = isRecord(command) ? command['command'] : undefined;
    if (commandId !== 'ripr.copyAgentRepairCommand') {
      continue;
    }
    const payload = commandPayload(action);
    if (payload && payload['label'] === 'agent_repair') {
      return payload;
    }
  }
  return undefined;
}

export function seamIdFromAgentRepairTarget(target: Record<string, unknown>): string | undefined {
  return nonEmptyString(target['seam_id']);
}

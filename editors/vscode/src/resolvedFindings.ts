/**
 * Findings the server reports as now exposed since the previous analysis
 * (#13636). When a saved test appears to observe a changed behavior, the gap's
 * diagnostic stops being published; without a note the editor just goes
 * quiet. The server owns the comparison (`resolved_findings` on
 * `ripr/analysisStatus`); this module only reads and words it.
 */

/** The typed `resolved_findings` member of `ripr/analysisStatus`. */
export interface AnalysisStatusResolvedFindings {
  count?: number;
  rows?: Array<{ text?: unknown } | null> | null;
  omitted?: number;
}

/** The parsed note: the server's row texts and the exact count. */
export interface ResolvedFindingsNote {
  count: number;
  lines: string[];
}

/**
 * Reads the server's rows. `undefined` when the payload is absent, malformed
 * or reports nothing, so callers stay silent rather than guessing.
 */
export function resolvedFindingsNote(value: unknown): ResolvedFindingsNote | undefined {
  if (!value || typeof value !== 'object') {
    return undefined;
  }
  const resolved = value as AnalysisStatusResolvedFindings;
  const count = typeof resolved.count === 'number' && Number.isInteger(resolved.count) ? resolved.count : 0;
  if (count <= 0) {
    return undefined;
  }
  const lines = (Array.isArray(resolved.rows) ? resolved.rows : [])
    .map((row) => (row && typeof row.text === 'string' ? row.text.trim() : ''))
    .filter((text) => text.length > 0);
  const omitted = count - lines.length;
  if (omitted > 0 && lines.length > 0) {
    lines.push(`+${omitted} more`);
  }
  return { count, lines };
}

/**
 * Whether to announce `note`, given the key of the last announced row set,
 * and the key to keep. The server recomputes the rows at each commit (a
 * repeat commit has none), so a re-sent status with the same rows is not
 * announced again, and a status with no rows resets the key.
 */
export function resolvedFindingsAnnouncement(
  lastKey: string | undefined,
  note: ResolvedFindingsNote | undefined
): { announce: boolean; key: string | undefined } {
  if (!note) {
    return { announce: false, key: undefined };
  }
  const key = `${note.count}\n${note.lines.join('\n')}`;
  return { announce: key !== lastKey, key };
}

/** The one-line message shown once per snapshot. */
export function resolvedFindingsMessage(note: ResolvedFindingsNote): string {
  const head = note.count === 1
    ? 'ripr: 1 changed behavior is now exposed since the last analysis: a test appears to observe it (static evidence).'
    : `ripr: ${note.count} changed behaviors are now exposed since the last analysis: tests appear to observe them (static evidence).`;
  const first = note.lines[0];
  return first && note.count === 1 ? `${head} ${first}` : head;
}

function merge(base: Record<string, number>, extra: Record<string, number>): Record<string, number> {
  const out: Record<string, number> = { ...base };
  for (const key of Object.keys(extra)) {
    out[key] = (out[key] ?? 0) + extra[key];
  }
  return out;
}

export default { merge };

export function parse(qs: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const pair of qs.split('&')) {
    const kv = pair.split('=');
    out[kv[0]] = kv[1] || '';
  }
  return out;
}

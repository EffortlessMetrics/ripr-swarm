export interface Flags {
  name: string;
  ok: boolean;
}

export function markOk(cfg: Flags): Flags {
  return { ...cfg, ok: true };
}

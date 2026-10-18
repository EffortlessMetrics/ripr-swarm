export interface Flags {
  name: string;
  ok?: boolean;
}

export function markBoth(cfg: Flags): Flags {
  return { ...cfg, ok: true };
}

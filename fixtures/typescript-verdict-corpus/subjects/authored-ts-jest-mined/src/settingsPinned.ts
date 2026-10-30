export interface Flags {
  name: string;
  ok?: boolean;
}

export function pinOk(cfg: Flags): Flags {
  return { ...cfg, ok: true };
}

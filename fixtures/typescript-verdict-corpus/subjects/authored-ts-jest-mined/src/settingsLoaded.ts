export interface Flags {
  name: string;
  ok: boolean;
}

export function loadFlags(): Flags {
  return { name: "bo", ok: true };
}

export function markLoaded(cfg: Flags): Flags {
  return { ...cfg, ok: true };
}

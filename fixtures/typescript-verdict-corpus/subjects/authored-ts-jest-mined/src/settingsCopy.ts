export interface Ready {
  ok: boolean;
  retries: number;
}

export function readyCopy(cfg: Ready): Ready {
  return cfg;
}

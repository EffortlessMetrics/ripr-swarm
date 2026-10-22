export interface Cfg {
  name: string;
  ok: boolean;
}

export function ready(cfg: Cfg): Cfg {
  return cfg;
}

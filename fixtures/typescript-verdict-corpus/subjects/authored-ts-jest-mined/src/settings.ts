export interface Settings {
  ready: boolean;
  retries: number;
}

export function readySettings(cfg: Settings): Settings {
  if (cfg.ready) {
    return cfg;
  }
  return { ...cfg, ready: true };
}

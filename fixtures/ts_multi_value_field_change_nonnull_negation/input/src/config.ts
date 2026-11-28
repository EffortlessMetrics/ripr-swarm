export interface Config {
  retries: number;
  verbose: boolean;
  timeout: number;
  enabled: boolean;
}

export function buildConfig(): Config {
  return { retries: 3, verbose: false, timeout: 30, enabled: true };
}

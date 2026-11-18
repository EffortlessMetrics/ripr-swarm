export interface Config {
  retries: number;
  verbose: boolean;
  timeout: number;
}

export function buildConfig(): Config {
  return { retries: 3, verbose: false, timeout: 30 };
}

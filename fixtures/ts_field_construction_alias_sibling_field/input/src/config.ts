export interface Config {
  retries: number;
  verbose: boolean;
}

export function buildConfig(): Config {
  return { retries: 3, verbose: false };
}

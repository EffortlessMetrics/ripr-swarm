export interface Config {
  retries: { n: number; other: number };
  verbose: boolean;
}

export function buildConfig(): Config {
  return { retries: { n: 3, other: 1 }, verbose: false };
}

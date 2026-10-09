export interface Options {
  ready: boolean;
  retries: number;
}

export function readyOptions(opts: Options): Options {
  if (opts.ready) {
    return opts;
  }
  return { ...opts, ready: true };
}

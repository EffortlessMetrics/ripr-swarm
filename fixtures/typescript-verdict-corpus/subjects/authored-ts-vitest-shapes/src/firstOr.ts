export function firstOr<T>(xs: readonly T[], fallback: T): T {
  return xs.length > 0 ? xs[0] : fallback;
}

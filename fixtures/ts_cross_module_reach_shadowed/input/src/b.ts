import { decodeThing } from "./a";

export function normalize(p: string, decodeThing: (s: string) => string): string {
  return "/" + decodeThing(p);
}

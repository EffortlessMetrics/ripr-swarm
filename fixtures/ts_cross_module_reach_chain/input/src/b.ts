import { decodeThing } from "./a";

export function normalize(p: string): string {
  return "/" + decodeThing(p);
}

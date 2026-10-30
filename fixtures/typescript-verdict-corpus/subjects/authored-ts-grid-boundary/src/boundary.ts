export function isAdult(age: number): boolean {
  return age >= 18;
}

export function freeShipping(cents: number): boolean {
  return cents >= 5000;
}

export function canRetire(age: number): boolean {
  return age >= 67;
}

export function passes(score: number): boolean {
  return score >= 50;
}

export function overweight(grams: number): boolean {
  return grams >= 2000;
}

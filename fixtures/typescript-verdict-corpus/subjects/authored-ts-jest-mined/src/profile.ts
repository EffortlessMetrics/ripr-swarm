export function buildProfile(name: string, age: number) {
  return {
    name,
    adult: age >= 18,
    tags: [] as string[],
  };
}

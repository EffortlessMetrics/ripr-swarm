export function buildProfileSpread(name: string, age: number, overrides: { adult?: boolean }) {
  return {
    name,
    adult: age >= 18,
    ...overrides,
  };
}

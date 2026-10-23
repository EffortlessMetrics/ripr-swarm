export function load(done: (value: number) => void): void {
  setTimeout(() => done(3), 0);
}

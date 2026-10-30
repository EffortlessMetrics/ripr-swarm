export async function loadOrDefault(read: () => Promise<number>, fallback: number): Promise<number> {
  try {
    return await read();
  } catch {
    return fallback;
  }
}

function createImpl(initial: number): { value: number; bump: () => number } {
  const state = { value: initial, bump: () => (state.value += 3) };
  return state;
}

export const createStore = (initial: number) => createImpl(initial);

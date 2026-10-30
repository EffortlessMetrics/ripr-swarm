export function minimum(limit: number) {
  return (target: object, key: string): void => {
    let stored = 0;
    Object.defineProperty(target, key, {
      get: () => stored,
      set: (value: number) => {
        if (value < limit) {
          throw new RangeError(`${key} below ${limit}`);
        }
        stored = value;
      },
    });
  };
}

export class Form {
  @minimum(3)
  quantity = 3;
}

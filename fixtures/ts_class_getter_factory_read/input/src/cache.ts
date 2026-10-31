export class Cached<T> {
  private _getter: (() => T) | undefined;
  private _value: T | undefined;

  constructor(getter: () => T) {
    this._getter = getter;
  }

  get value(): T {
    const getter = this._getter;
    if (getter !== undefined) {
      this._value = getter();
      this._getter = undefined;
    }
    return this._value as T;
  }
}

export function cached<T>(getter: () => T): Cached<T> {
  return new Cached(getter);
}

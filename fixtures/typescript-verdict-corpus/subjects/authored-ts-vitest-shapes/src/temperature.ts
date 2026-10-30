export class Temperature {
  constructor(readonly celsius: number) {}

  static fromFahrenheit(f: number): Temperature {
    return new Temperature(Math.round(((f - 32) * 5) / 9));
  }
}

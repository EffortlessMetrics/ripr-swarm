export class Url {
  readonly scheme: string;
  constructor(input: string) {
    this.scheme = input.split(":")[0].toLowerCase();
  }
}

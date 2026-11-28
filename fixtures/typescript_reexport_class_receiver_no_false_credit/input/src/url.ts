export class Url {
    protocol: string;
    constructor(input = '') {
        this.protocol = input.split(':')[0];
    }
    get origin(): string {
        return this.protocol + '://host';
    }
}

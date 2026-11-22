export function serialize(input: unknown): string {
    return new Serializer().serialize(input);
}

function compareStrings(a: string, b: string): number {
    if (a === b) {
        return 0;
    }
    return a < b ? -1 : 1;
}

const Serializer = /*@__PURE__*/ (function () {
    class Serializer {
        serialize(value: unknown): string {
            if (value === null || typeof value !== 'object') {
                return String(value);
            }
            const record = value as Record<string, unknown>;
            const keys = Object.keys(record).sort(compareStrings);
            return '{' + keys.map((key) => key + ':' + this.serialize(record[key])).join(',') + '}';
        }
    }
    return Serializer;
})();

function compareLengths(a: string, b: string): number {
    return a.length - b.length;
}

export function later(keys: string[]): string[] {
    setTimeout(compareLengths);
    return keys;
}

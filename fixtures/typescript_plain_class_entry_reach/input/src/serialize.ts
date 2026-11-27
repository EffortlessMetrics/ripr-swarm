export function serialize(input: Record<string, number>): string {
    return new Serializer().write(input);
}

function compareKeys(a: string, b: string): number {
    if (a === b) {
        return 0;
    }
    return a < b ? -1 : 1;
}

class Serializer {
    write(record: Record<string, number>): string {
        const keys = this.order(Object.keys(record));
        return keys.map((key) => key + ':' + record[key]).join(',');
    }

    order(keys: string[]): string[] {
        return keys.sort(compareKeys);
    }

    describe(keys: string[]): string {
        return keys.join('+');
    }
}

export function count(Counter: any, keys: string[]): number {
    return new Counter().total(keys);
}

class Counter {
    total(keys: string[]): number {
        return keys.length * 2;
    }
}

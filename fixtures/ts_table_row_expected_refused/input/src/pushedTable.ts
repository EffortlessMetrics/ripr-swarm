export function pushedTable(points: number): string {
    if (points > 1000) {
        return "gold";
    }
    return "silver";
}

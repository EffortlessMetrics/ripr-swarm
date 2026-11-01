export function derivedTable(points: number): string {
    if (points > 1000) {
        return "gold";
    }
    return "silver";
}

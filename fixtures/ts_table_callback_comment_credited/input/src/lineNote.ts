export function lineNote(points: number): string {
    if (points >= 1000) {
        return "gold";
    }
    return "silver";
}

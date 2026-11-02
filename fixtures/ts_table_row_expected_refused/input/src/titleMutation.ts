export function titleMutation(point: { points: number }): string {
    if (point.points > 1000) {
        return "gold";
    }
    return "silver";
}

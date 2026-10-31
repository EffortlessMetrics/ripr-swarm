export function moduleConstant(score: number): string {
    if (score > 50) {
        return "gold";
    }
    return "basic";
}

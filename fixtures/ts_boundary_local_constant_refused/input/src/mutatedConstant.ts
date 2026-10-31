export function mutatedConstant(score: number): string {
    if (score > 50) {
        return "gold";
    }
    return "basic";
}

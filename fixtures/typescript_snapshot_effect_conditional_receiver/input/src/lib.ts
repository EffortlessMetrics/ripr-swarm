export class Outbox {
    private lines: string[] = [];

    send(id: number): string[] {
        this.lines.push("shipped " + id);
        return this.lines;
    }
}

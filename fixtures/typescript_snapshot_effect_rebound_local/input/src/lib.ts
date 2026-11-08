export function enqueue(queue: string[], id: number): string[] {
    queue.push("shipped " + id);
    return queue;
}

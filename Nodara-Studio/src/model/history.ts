/** Bounded undo/redo stack for serialized workflow snapshots. */
export class WorkflowHistory {
  private entries: string[];
  private index: number;

  constructor(initial: string, private readonly limit = 100) {
    this.entries = [initial];
    this.index = 0;
  }

  reset(snapshot: string): void {
    this.entries = [snapshot];
    this.index = 0;
  }

  push(snapshot: string): void {
    if (snapshot === this.entries[this.index]) return;
    this.entries = this.entries.slice(0, this.index + 1);
    this.entries.push(snapshot);
    if (this.entries.length > this.limit) {
      this.entries.shift();
    }
    this.index = this.entries.length - 1;
  }

  undo(): string | null {
    if (this.index <= 0) return null;
    this.index -= 1;
    return this.entries[this.index];
  }

  redo(): string | null {
    if (this.index >= this.entries.length - 1) return null;
    this.index += 1;
    return this.entries[this.index];
  }

  canUndo(): boolean {
    return this.index > 0;
  }

  canRedo(): boolean {
    return this.index < this.entries.length - 1;
  }
}

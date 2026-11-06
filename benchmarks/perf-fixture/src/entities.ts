// oxlint-disable typescript/prefer-for-of max-classes-per-file -- no

export interface Identified {
	readonly id: number;
}

export class Entity implements Identified {
	private tags: Array<string> = [];

	public readonly id: number;

	public name: string;

	public constructor(id: number, name: string) {
		this.id = id;
		this.name = name;
	}

	public addTag(tag: string): void {
		this.tags.push(tag);
	}

	public describe(): string {
		return `${this.id}:${this.name}`;
	}

	public hasTag(tag: string): boolean {
		for (let index = 0; index < this.tags.length; index += 1) {
			if (this.tags[index] === tag) return true;
		}
		return false;
	}
}

export class NamedEntity extends Entity {
	public readonly displayName: string;

	public constructor(id: number, name: string, displayName: string) {
		super(id, name);
		this.displayName = displayName;
	}

	public describe(): string {
		return `${super.describe()} (${this.displayName})`;
	}
}

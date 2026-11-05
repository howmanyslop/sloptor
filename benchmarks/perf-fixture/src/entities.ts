export interface Identified {
	readonly id: number;
}

export class Entity implements Identified {
	public readonly id: number;
	public name: string;
	private tags: string[] = [];

	public constructor(id: number, name: string) {
		this.id = id;
		this.name = name;
	}

	public addTag(tag: string): void {
		this.tags.push(tag);
	}

	public hasTag(tag: string): boolean {
		for (let i = 0; i < this.tags.length; i++) {
			if (this.tags[i] === tag) return true;
		}
		return false;
	}

	public describe(): string {
		return `${this.id}:${this.name}`;
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

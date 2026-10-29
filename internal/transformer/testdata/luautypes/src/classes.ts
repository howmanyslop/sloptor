// Placeholder for WP7: a base class with a static, a derived class with an
// override, and a generic class.
export class Animal {
	constructor(public name: string) {}

	speak(): string {
		return this.name;
	}

	static create(name: string): Animal {
		return new Animal(name);
	}
}

export class Dog extends Animal {
	constructor(name: string, public tricks: number) {
		super(name);
	}

	speak(): string {
		return `${this.name}!`;
	}
}

export class Box<T> {
	constructor(public value: T) {}

	get(): T {
		return this.value;
	}
}

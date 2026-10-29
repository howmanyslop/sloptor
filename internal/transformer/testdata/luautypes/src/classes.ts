// Classes (plan WP7): a base class with a static and a toString, a derived
// class with an override and a super call, an abstract base with a concrete
// subclass, a generic class with a generic method and a generic subclass, a
// class referenced before its declaration (hoisted), class expressions, and
// an imported class constructed through its mirrored alias.
import { Circle } from "./lib/shapes";

export class Animal {
	static count = 0;

	private secret = 1;

	readonly tag?: string;

	constructor(public name: string) {
		Animal.count += 1;
	}

	speak(): string {
		return this.name;
	}

	toString(): string {
		return `Animal(${this.name})`;
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

	both(): string {
		return `${this.speak()} ${this.tricks}`;
	}
}

export class Puppy extends Dog {}

export abstract class Figure {
	abstract area(): number;

	describe(): string {
		return `${this.area()}`;
	}
}

export class Square extends Figure {
	constructor(private side: number) {
		super();
	}

	area(): number {
		return this.side * this.side;
	}
}

export class Box<T> {
	constructor(public value: T) {}

	get(): T {
		return this.value;
	}

	map<U>(transform: (value: T) => U): U {
		return transform(this.value);
	}
}

export class Pair<T> extends Box<T> {
	constructor(value: T, public other: T) {
		super(value);
	}

	swap(): void {
		const value = this.value;
		this.value = this.other;
		this.other = value;
	}
}

export class NumberBox extends Box<number> {
	double(): number {
		return this.get() * 2;
	}
}

export function makeLate(): Late {
	return new Late(1);
}

export class Late {
	constructor(public n: number) {}
}

export const Named = class Inner {
	constructor(public id: number) {}

	twice(): number {
		return this.id * 2;
	}
};

export const Anonymous = class {
	label = "anonymous";
};

export function unitCircle(): Circle {
	return new Circle(1);
}

export function useAll(): number {
	const dog = new Dog("rex", 2);
	const animal: Animal = dog;
	const square: Figure = new Square(3);
	const pair = new Pair(1, 2);
	pair.swap();
	// A generic method inherited by a derived class is called through the
	// base type: Luau's new solver cannot instantiate `map`'s `U` when the
	// receiver is a strict subtype of the `self` the alias declares.
	const box: Box<number> = pair;
	const named = new Named(4);
	const anonymous = new Anonymous();
	return (
		animal.speak().size() +
		dog.both().size() +
		square.describe().size() +
		Animal.create("a").name.size() +
		box.get() +
		box.map((value) => value + 1) +
		new NumberBox(2).double() +
		makeLate().n +
		named.twice() +
		anonymous.label.size() +
		Puppy.count +
		unitCircle().radius
	);
}

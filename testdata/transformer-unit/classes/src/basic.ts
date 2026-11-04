class Animal {
	name: string;
	legs = 4;
	constructor(name: string) {
		this.name = name;
	}
	walk(dist: number) {
		print(this.name, dist);
	}
	static create() {
		return new Animal("a");
	}
	static VERSION = 1;
}

class Plain {}

const p = new Plain();
const a = Animal.create();
a.walk(Animal.VERSION);
print(p);

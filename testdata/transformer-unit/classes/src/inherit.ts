class Base {
	constructor(
		public x: number,
		private readonly y = 2,
	) {
		print("base", x, y);
	}
	toString() {
		return "Base";
	}
	m() {
		return this.x;
	}
}

class Derived extends Base {
	z: number;
	constructor() {
		print("before super");
		super(1);
		this.z = 3;
		print("after super");
	}
	m() {
		return super.m() + 1;
	}
}

class NoCtor extends Base {}

abstract class Abs {
	abstract a(): void;
	b() {
		print("b");
	}
}

abstract class AbsExtends extends Base {
	abstract c(): void;
}

class FromAbs extends Abs {
	a() {}
}

print(new Derived(), new NoCtor(2), new FromAbs(), tostring(new Base(1)));

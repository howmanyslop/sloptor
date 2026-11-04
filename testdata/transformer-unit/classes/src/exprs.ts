const Foo = class Inner {
	m() {
		return Inner;
	}
};

const Bar = class {
	m() {
		return 1;
	}
};

class Statics {
	static counter = 0;
	static {
		Statics.counter = 5;
		print(this.counter);
	}
}

const KEY = "dyn";

class Computed {
	[KEY]() {
		return 1;
	}
}

export default class {
	m() {}
}

print(Foo, Bar, Statics, Computed);

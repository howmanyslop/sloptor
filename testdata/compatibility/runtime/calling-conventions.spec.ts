export = () => {
	it("keeps arguments aligned for instantiated callback and method aliases", () => {
		type Callable<T> = (this: T, value: number) => number;
		function original(value: number) {
			return value + 1;
		}
		const callback: Callable<void> = original;
		const object: { value: number; callback: Callable<void>; method: Callable<{ value: number }> } = {
			value: 10,
			callback,
			method(value) {
				return this.value + value;
			},
		};
		const optional: typeof object | undefined = (() => object)();
		expect(callback(41)).to.equal(42);
		expect(object.callback(41)).to.equal(42);
		expect(object["callback"](41)).to.equal(42);
		expect(optional?.callback(41)).to.equal(42);
		expect(object.callback?.(41)).to.equal(42);
		expect(object.method(32)).to.equal(42);
		expect(object["method"](32)).to.equal(42);
		expect(optional?.method(32)).to.equal(42);
		expect(object.method?.(32)).to.equal(42);
	});

	it("supplies the receiver slot for direct, optional, and spread calls", () => {
		function direct(this: defined | void, value: number) {
			return value + 1;
		}
		function spread(this: defined | void, ...values: number[]) {
			return values[0] + values[1];
		}
		const optional: typeof direct | undefined = (() => direct)();
		const optionalSpread: typeof spread | undefined = (() => spread)();
		const values = [20, 22];
		expect(direct(41)).to.equal(42);
		expect(optional?.(41)).to.equal(42);
		expect(spread(...values)).to.equal(42);
		expect(optionalSpread?.(...values)).to.equal(42);
	});

	it("evaluates a generic callee before spread arguments and skips absent calls", () => {
		type Callable<T> = (this: T, ...values: number[]) => number;
		const order = new Array<string>();
		const callback: Callable<void> = (...values) => values[0] + values[1];
		function getCallback() {
			order.push("callee");
			return callback;
		}
		function getValues(): LuaTuple<[number, number]> {
			order.push("arguments");
			return $tuple(20, 22);
		}
		expect(getCallback()(...getValues())).to.equal(42);
		expect(order.join(",")).to.equal("callee,arguments");
		const absent = ((): Callable<void> | undefined => undefined)();
		expect(absent?.(...getValues())).to.equal(undefined);
		expect(order.join(",")).to.equal("callee,arguments");
	});

	it("accepts generic receivers that cannot become void", () => {
		function constrained<T extends defined>() {
			return {
				method(this: T, value: number) {
					return value + 1;
				},
			};
		}
		function fixedUnion<T>() {
			return {
				method(this: T | defined, value: number) {
					return value + 1;
				},
			};
		}
		function excludesVoid<T>() {
			return {
				method(this: Exclude<T, void>, value: number) {
					return value + 1;
				},
				wrapped(this: [T] extends [void] ? never : T, value: number) {
					return value + 1;
				},
			};
		}
		expect(constrained<defined>().method(41)).to.equal(42);
		expect(fixedUnion<void>().method(41)).to.equal(42);
		const object = excludesVoid<defined | void>();
		expect(object.method(41)).to.equal(42);
		expect(object.wrapped(41)).to.equal(42);
	});

	it("infers receivers for contextual function expressions in collections", () => {
		const callbacks: Array<(this: defined, value?: number) => number> = [
			function () {
				return 42;
			},
			function (value) {
				return value === undefined ? 0 : value;
			},
		];
		expect(callbacks[0](42)).to.equal(42);
		expect(callbacks[1](42)).to.equal(42);
	});

	it("omits receivers for this-void super methods", () => {
		class Base {
			method(this: void, value: number) {
				return value + 1;
			}
		}
		class Derived extends Base {
			property() {
				return super.method(41);
			}
			element() {
				return super["method"](41);
			}
		}
		const object = new Derived();
		expect(object.property()).to.equal(42);
		expect(object.element()).to.equal(42);
	});
};

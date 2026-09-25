export = () => {
	it("consumes omitted Set elements before collecting rest", () => {
		const [, ...rest] = new Set([10, 20, 30]);
		expect(rest.size()).to.equal(2);
	});

	it("exhausts a one-element Set through an omitted element", () => {
		const [, second] = new Set([10]);
		expect(second).to.equal(undefined);
	});

	it("leaves empty Set elements undefined and rest empty", () => {
		const [first, ...rest] = new Set<number>();
		expect(first).to.equal(undefined);
		expect(rest.size()).to.equal(0);
	});

	it("does not restart an exhausted Set", () => {
		const [first, , third, ...rest] = new Set([10]);
		expect(first).to.equal(10);
		expect(third).to.equal(undefined);
		expect(rest.size()).to.equal(0);
	});

	it("consumes omitted Map entries before collecting rest", () => {
		const [, ...rest] = new Map([
			[1, "one"],
			[2, "two"],
			[3, "three"],
		]);
		expect(rest.size()).to.equal(2);
		expect(rest[0][0] === rest[1][0]).to.equal(false);
	});

	it("leaves empty Map elements undefined and rest empty", () => {
		const [first, ...rest] = new Map<number, string>();
		expect(first).to.equal(undefined);
		expect(rest.size()).to.equal(0);
	});

	it("does not restart an exhausted Map", () => {
		const [first, , third, ...rest] = new Map([[10, "ten"]]);
		expect(first[0]).to.equal(10);
		expect(third).to.equal(undefined);
		expect(rest.size()).to.equal(0);
	});

	it("continues generator rest after an omitted prefix", () => {
		function* values() {
			yield 10;
			yield 20;
			yield 30;
		}
		const [, ...rest] = values();
		expect(rest.size()).to.equal(2);
		expect(rest[0]).to.equal(20);
		expect(rest[1]).to.equal(30);
	});

	it("does not expose generator completion values or resume exhaustion", () => {
		function* values() {
			yield 10;
			return 99;
		}
		const [first, second, third, ...rest] = values();
		expect(first).to.equal(10);
		expect(second).to.equal(undefined);
		expect(third).to.equal(undefined);
		expect(rest.size()).to.equal(0);
	});

	it("does not call next after an empty generator reports completion", () => {
		const events = new Array<string>();
		function* values() {
			return 99;
		}
		const iterator = values();
		const originalNext = iterator.next;
		iterator.next = () => {
			events.push("next");
			return originalNext();
		};
		const [first, , ...rest] = iterator;
		expect(first).to.equal(undefined);
		expect(rest.size()).to.equal(0);
		expect(events.join(",")).to.equal("next");
	});

	it("collects every element with rest-only patterns", () => {
		function* values() {
			yield 10;
			yield 20;
		}
		const [...setRest] = new Set([10, 20, 30]);
		const [...mapRest] = new Map([
			[10, "ten"],
			[20, "twenty"],
		]);
		const [...generatorRest] = values();
		expect(setRest.size()).to.equal(3);
		expect(mapRest.size()).to.equal(2);
		expect(generatorRest.size()).to.equal(2);
		expect(generatorRest[0]).to.equal(10);
		expect(generatorRest[1]).to.equal(20);
	});

	it("evaluates computed destinations before iterator reads and defaults", () => {
		const events = new Array<string>();
		const target = new Array<number>();
		function destination() {
			events.push("destination");
			return 0;
		}
		function fallback() {
			events.push("default");
			return 42;
		}
		function* values() {
			events.push("next");
			yield undefined;
		}
		[target[destination()] = fallback()] = values();
		expect(events.join(",")).to.equal("destination,next,default");
		expect(target[0]).to.equal(42);
	});

	it("captures the destination before a default changes its index", () => {
		let index = 0;
		const target = [10, 20];
		function fallback() {
			index = 1;
			return 42;
		}
		[target[index] = fallback()] = new Set<number>();
		expect(target[0]).to.equal(42);
		expect(target[1]).to.equal(20);
	});

	it("keeps Set rest valid when a destination removes the consumed key", () => {
		const values = new Set([10, 20, 30, 40]);
		let first = 0;
		let second = 0;
		const target = new Array<Array<number>>();
		function destination() {
			values.delete(second);
			return 0;
		}
		[first, second, ...target[destination()]] = values;
		expect(target[0].size()).to.equal(2);
		expect(target[0].includes(first)).to.equal(false);
		expect(target[0].includes(second)).to.equal(false);
	});

	it("evaluates rest destinations before consuming the iterator", () => {
		const values = new Set([10, 20, 30]);
		const target = new Array<Array<number>>();
		function destination() {
			values.clear();
			return 0;
		}
		[, ...target[destination()]] = values;
		expect(target[0].size()).to.equal(0);
	});

	it("preserves existing array and object rest", () => {
		const [first, ...rest] = [10, 20, 30];
		const { x, ...remaining } = { x: 1, y: 2 };
		expect(first).to.equal(10);
		expect(rest.size()).to.equal(2);
		expect(rest[0]).to.equal(20);
		expect(x).to.equal(1);
		expect(remaining.y).to.equal(2);
	});
};

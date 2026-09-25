export = () => {
	it("checks a shrinking array before every iteration", () => {
		const values = [1, 2, 3, 4];
		let iterations = 0;
		for (let i = 0; i < values.size(); i++) {
			values.pop();
			iterations++;
		}
		expect(iterations).to.equal(2);
	});

	it("uses the variable named by the condition", () => {
		let other = 2;
		let iterations = 0;
		for (let i = 0; other < 3; i++) {
			other++;
			iterations++;
		}
		expect(iterations).to.equal(1);
	});

	it("decrements the variable named by the incrementor", () => {
		let other = 2;
		for (let i = 2; i >= 0; other -= 1) {
			if (other === 0) break;
		}
		expect(other).to.equal(0);
	});

	it("allows a zero step until the body breaks", () => {
		let iterations = 0;
		for (let i = 0; i < 2; i += 0) {
			iterations++;
			if (iterations === 3) break;
		}
		expect(iterations).to.equal(3);
	});

	it("keeps fractional bounds and steps", () => {
		let boundIterations = 0;
		for (let i = 0; i < 2 ** -1; i++) boundIterations++;
		let stepIterations = 0;
		for (let i = 0; i < 2; i += 0.5) stepIterations++;
		expect(boundIterations).to.equal(1);
		expect(stepIterations).to.equal(4);
	});

	it("evaluates effectful literal-typed bounds each time", () => {
		let reads = 0;
		function bound(): 3 {
			reads++;
			return 3;
		}
		let iterations = 0;
		for (let i = 0; i < bound(); i++) iterations++;
		expect(iterations).to.equal(3);
		expect(reads).to.equal(4);
	});

	it("reads changing literal-typed bounds each time", () => {
		const bounds: { value: 1 | 3 } = { value: 3 };
		let iterations = 0;
		for (let i = 0; i < bounds.value; i++) {
			bounds.value = 1;
			iterations++;
		}
		expect(iterations).to.equal(1);
	});

	it("carries body assignments into the next iteration", () => {
		let iterations = 0;
		for (let i = 0; i < 4; i++) {
			i++;
			iterations++;
		}
		expect(iterations).to.equal(2);
	});

	it("carries array destructuring writes through continue", () => {
		let iterations = 0;
		for (let i = 0; i < 4; i++) {
			[i] = [i + 1];
			iterations++;
			continue;
		}
		expect(iterations).to.equal(2);
	});

	it("carries object destructuring writes into the next iteration", () => {
		let renamed = 0;
		for (let i = 0; i < 4; i++) {
			({ value: i } = { value: i + 1 });
			renamed++;
		}
		let shorthand = 0;
		for (let i = 0; i < 4; i++) {
			({ i } = { i: i + 1 });
			shorthand++;
		}
		expect(renamed).to.equal(2);
		expect(shorthand).to.equal(2);
	});

	it("keeps integer constant expressions in both directions", () => {
		const first = -(1 + 1);
		const last = first + 8;
		const step = 1 + 1;
		let ascending = 0;
		for (let i = first; i < last; i += step) ascending += i;
		let descending = 0;
		for (let i = last; i > first; i -= step) descending += i;
		expect(ascending).to.equal(4);
		expect(descending).to.equal(12);
	});
};

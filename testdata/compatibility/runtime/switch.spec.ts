export = () => {
	it("compares cases against the operand before a case increments it", () => {
		let operand = 1;
		let result = "missing";
		switch (operand) {
			case operand++:
				result = "original";
				break;
			default:
				result = "changed";
		}
		expect(result).to.equal("original");
		expect(operand).to.equal(2);
	});

	it("keeps the original operand across a mutating case call", () => {
		let operand = 2;
		let result = "missing";
		function changeOperand() {
			operand = 3;
			return 1;
		}
		switch (operand) {
			case changeOperand():
				result = "first";
				break;
			case 2:
				result = "original";
				break;
			default:
				result = "changed";
		}
		expect(result).to.equal("original");
		expect(operand).to.equal(3);
	});

	it("evaluates the operand once before cases and stops searching after a match", () => {
		let events = "";
		function operand() {
			events += "operand;";
			return 2;
		}
		function label(value: number) {
			events += `case${value};`;
			return value;
		}
		switch (operand()) {
			case label(1):
				events += "wrong;";
				break;
			case label(2):
				events += "matched;";
				break;
			case label(3):
				events += "wrong;";
				break;
		}
		expect(events).to.equal("operand;case1;case2;matched;");
	});

	it("skips case calls and increment prerequisites during fallthrough", () => {
		let events = "";
		let nextCase = 10;
		function label() {
			events += "case;";
			return 2;
		}
		switch (1) {
			case 1:
				events += "first;";
			case label():
				events += "second;";
			case nextCase++:
				events += "third;";
				break;
			default:
				events += "default;";
		}
		expect(events).to.equal("first;second;third;");
		expect(nextCase).to.equal(10);
	});

	it("continues falling through after the body changes the operand", () => {
		let operand = 1;
		let result = "";
		switch (operand) {
			case 1:
				operand = 3;
				result += "first;";
			case 2:
				result += "second;";
			default:
				result += "default;";
		}
		expect(result).to.equal("first;second;default;");
		expect(operand).to.equal(3);
	});

	it("selects grouped literal cases and the final default", () => {
		function classify(value: number) {
			switch (value) {
				case 1:
				case 2:
					return "small";
				case 3:
					return "three";
				default:
					return "other";
			}
		}
		expect(classify(1)).to.equal("small");
		expect(classify(2)).to.equal("small");
		expect(classify(3)).to.equal("three");
		expect(classify(4)).to.equal("other");
	});

	it("captures the operand before a case binding shadows its name", () => {
		let operand = 2;
		let result = 0;
		switch (operand) {
			case 1:
				let operand = 99;
				result = operand;
				break;
			case 2:
				operand = 7;
				result = operand;
				break;
		}
		expect(result).to.equal(7);
		expect(operand).to.equal(2);
	});
};

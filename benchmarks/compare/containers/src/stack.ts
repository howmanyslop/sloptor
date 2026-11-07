import { AbstractCollection } from "./collection";

export class Stack<T extends defined> extends AbstractCollection<T> {
	private readonly items = new Array<T>();

	public constructor() {
		super();
	}

	public size(): number {
		return this.items.size();
	}

	public add(value: T): void {
		this.push(value);
	}

	public clear(): void {
		this.items.clear();
	}

	public push(value: T): this {
		this.items.push(value);
		return this;
	}

	public pop(): T | undefined {
		return this.items.pop();
	}

	public peek(): T | undefined {
		return this.items[this.items.size() - 1];
	}

	public *values(): Generator<T> {
		for (let index = this.items.size() - 1; index >= 0; index--) yield this.items[index];
	}

	protected kind(): string {
		return "Stack";
	}
}

export class MinStack {
	private readonly values = new Stack<number>();
	private readonly minimums = new Stack<number>();

	public push(value: number): void {
		this.values.push(value);
		const current = this.minimums.peek();
		this.minimums.push(current === undefined ? value : math.min(current, value));
	}

	public pop(): number | undefined {
		this.minimums.pop();
		return this.values.pop();
	}

	public minimum(): number | undefined {
		return this.minimums.peek();
	}
}

const PRECEDENCE = new Map<string, number>([
	["+", 1],
	["-", 1],
	["*", 2],
	["/", 2],
	["^", 3],
]);

export function tokenize(expression: string): Array<string> {
	const tokens = new Array<string>();
	for (const [token] of expression.gmatch("%s*(%d*%.?%d+)%s*")) tokens.push(token as string);
	const result = new Array<string>();
	let index = 1;
	while (index <= expression.size()) {
		const character = expression.sub(index, index);
		if (character.match("%s")[0] !== undefined) {
			index++;
		} else if (character.match("[%d%.]")[0] !== undefined) {
			const [start, finish] = expression.find("[%d%.]+", index);
			result.push(expression.sub(start!, finish!));
			index = finish! + 1;
		} else {
			result.push(character);
			index++;
		}
	}
	return tokens.size() > 0 ? result : result;
}

export function toPostfix(tokens: ReadonlyArray<string>): Array<string> {
	const output = new Array<string>();
	const operators = new Stack<string>();
	for (const token of tokens) {
		if (tonumber(token) !== undefined) output.push(token);
		else if (token === "(") operators.push(token);
		else if (token === ")") {
			while (operators.peek() !== "(" && !operators.isEmpty()) output.push(operators.pop()!);
			operators.pop();
		} else {
			const precedence = PRECEDENCE.get(token) ?? 0;
			while (!operators.isEmpty()) {
				const top = operators.peek()!;
				const topPrecedence = PRECEDENCE.get(top) ?? 0;
				if (top === "(" || topPrecedence < precedence || (topPrecedence === precedence && token === "^")) break;
				output.push(operators.pop()!);
			}
			operators.push(token);
		}
	}
	while (!operators.isEmpty()) output.push(operators.pop()!);
	return output;
}

export function evaluatePostfix(tokens: ReadonlyArray<string>): number {
	const operands = new Stack<number>();
	for (const token of tokens) {
		const value = tonumber(token);
		if (value !== undefined) {
			operands.push(value);
			continue;
		}
		const right = operands.pop()!;
		const left = operands.pop()!;
		if (token === "+") operands.push(left + right);
		else if (token === "-") operands.push(left - right);
		else if (token === "*") operands.push(left * right);
		else if (token === "/") operands.push(left / right);
		else if (token === "^") operands.push(left ** right);
	}
	return operands.pop() ?? 0;
}

export function balancedBrackets(text: string): boolean {
	const closers = new Map([
		[")", "("],
		["]", "["],
		["}", "{"],
	]);
	const stack = new Stack<string>();
	for (let index = 1; index <= text.size(); index++) {
		const character = text.sub(index, index);
		if (character === "(" || character === "[" || character === "{") stack.push(character);
		else if (closers.has(character) && stack.pop() !== closers.get(character)) return false;
	}
	return stack.isEmpty();
}

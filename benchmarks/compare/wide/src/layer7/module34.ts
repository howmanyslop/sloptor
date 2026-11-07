import { computeL6M05, L6M05Kind, BaseL6M05 } from "../layer6/module05";

export enum L7M34Kind {
	Done,
	Failed,
	Locked,
	Active,
}

export const L7M34_FACTOR = 4;

export class BaseL7M34 {
	protected total = 73;
	public kind = L7M34Kind.Done;

	public weigh(value: number): number {
		this.total += value * L7M34_FACTOR;
		return this.total;
	}

	public advance(): L7M34Kind {
		const order = [L7M34Kind.Done, L7M34Kind.Failed, L7M34Kind.Locked, L7M34Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class AnchorL7M34 extends BaseL7M34 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 8) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL7M34(input: number): number {
	let result = input * 4 + 73;
	result += computeL6M05(input % 5);
	return result % 16195;
}

export const L7M34_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 73,
	chain: (value) => computeL7M34(value),
};

export function linkL7M34(): Array<number> {
	const results = new Array<number>();
	const instanceL6M05 = new BaseL6M05();
	instanceL6M05.advance();
	results.push(instanceL6M05.read() + L6M05Kind.Idle);
	return results;
}

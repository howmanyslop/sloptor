import { computeL6M28, L6M28Kind, BaseL6M28 } from "../layer6/module28";
import { computeL0M00, L0M00Kind, BaseL0M00 } from "../layer0/module00";

export enum L7M03Kind {
	Queued,
	Idle,
	Failed,
}

export const L7M03_FACTOR = 4;

export class BaseL7M03 {
	protected total = 71;
	public kind = L7M03Kind.Queued;

	public weigh(value: number): number {
		this.total += value * L7M03_FACTOR;
		return this.total;
	}

	public advance(): L7M03Kind {
		const order = [L7M03Kind.Queued, L7M03Kind.Idle, L7M03Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class AnchorL7M03 extends BaseL6M28 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 10) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL7M03(input: number): number {
	let result = input * 4 + 71;
	result += computeL6M28(input % 5);
	result += computeL0M00(input % 11);
	return result % 9712;
}

export const L7M03_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 71,
	chain: (value) => computeL7M03(value),
};

export function linkL7M03(): Array<number> {
	const results = new Array<number>();
	const instanceL6M28 = new BaseL6M28();
	instanceL6M28.advance();
	results.push(instanceL6M28.read() + L6M28Kind.Failed);
	const instanceL0M00 = new BaseL0M00();
	instanceL0M00.advance();
	results.push(instanceL0M00.read() + L0M00Kind.Done);
	return results;
}

import { computeL6M13, L6M13Kind, BaseL6M13 } from "../layer6/module13";
import { computeL0M33, L0M33Kind, BaseL0M33 } from "../layer0/module33";

export enum L7M18Kind {
	Idle,
	Failed,
	Queued,
}

export const L7M18_FACTOR = 17;

export class BaseL7M18 {
	protected total = 12;
	public kind = L7M18Kind.Idle;

	public mix(value: number): number {
		this.total += value * L7M18_FACTOR;
		return this.total;
	}

	public advance(): L7M18Kind {
		const order = [L7M18Kind.Idle, L7M18Kind.Failed, L7M18Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class WidgetL7M18 extends BaseL7M18 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 13) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL7M18(input: number): number {
	let result = input * 17 + 12;
	result += computeL6M13(input % 4);
	result += computeL0M33(input % 4);
	return result % 50817;
}

export const L7M18_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 12,
	chain: (value) => computeL7M18(value),
};

export function linkL7M18(): Array<number> {
	const results = new Array<number>();
	const instanceL6M13 = new BaseL6M13();
	instanceL6M13.advance();
	results.push(instanceL6M13.read() + L6M13Kind.Failed);
	const instanceL0M33 = new BaseL0M33();
	instanceL0M33.advance();
	results.push(instanceL0M33.read() + L0M33Kind.Idle);
	return results;
}

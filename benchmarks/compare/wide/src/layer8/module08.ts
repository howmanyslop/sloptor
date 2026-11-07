import { computeL7M24, L7M24Kind, BaseL7M24 } from "../layer7/module24";
import { computeL5M31, L5M31Kind, BaseL5M31 } from "../layer5/module31";

export enum L8M08Kind {
	Locked,
	Done,
	Paused,
}

export const L8M08_FACTOR = 9;

export class BaseL8M08 {
	protected total = 58;
	public kind = L8M08Kind.Locked;

	public blend(value: number): number {
		this.total += value * L8M08_FACTOR;
		return this.total;
	}

	public advance(): L8M08Kind {
		const order = [L8M08Kind.Locked, L8M08Kind.Done, L8M08Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LedgerL8M08 extends BaseL7M24 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 9) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL8M08(input: number): number {
	let result = input * 9 + 58;
	result += computeL7M24(input % 4);
	result += computeL5M31(input % 7);
	return result % 31723;
}

export const L8M08_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 58,
	chain: (value) => computeL8M08(value),
};

export function linkL8M08(): Array<number> {
	const results = new Array<number>();
	const instanceL7M24 = new BaseL7M24();
	instanceL7M24.advance();
	results.push(instanceL7M24.read() + L7M24Kind.Idle);
	const instanceL5M31 = new BaseL5M31();
	instanceL5M31.advance();
	results.push(instanceL5M31.read() + L5M31Kind.Queued);
	return results;
}

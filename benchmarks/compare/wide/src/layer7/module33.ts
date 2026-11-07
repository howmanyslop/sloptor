import { computeL6M38, L6M38Kind, BaseL6M38 } from "../layer6/module38";
import { computeL4M02, L4M02Kind, BaseL4M02 } from "../layer4/module02";

export enum L7M33Kind {
	Queued,
	Locked,
	Idle,
	Active,
	Failed,
}

export const L7M33_FACTOR = 2;

export class BaseL7M33 {
	protected total = 42;
	public kind = L7M33Kind.Queued;

	public fold(value: number): number {
		this.total += value * L7M33_FACTOR;
		return this.total;
	}

	public advance(): L7M33Kind {
		const order = [L7M33Kind.Queued, L7M33Kind.Locked, L7M33Kind.Idle, L7M33Kind.Active, L7M33Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class AnchorL7M33 extends BaseL7M33 {
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

export function computeL7M33(input: number): number {
	let result = input * 2 + 42;
	result += computeL6M38(input % 7);
	result += computeL4M02(input % 11);
	return result % 29715;
}

export const L7M33_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 42,
	chain: (value) => computeL7M33(value),
};

export function linkL7M33(): Array<number> {
	const results = new Array<number>();
	const instanceL6M38 = new BaseL6M38();
	instanceL6M38.advance();
	results.push(instanceL6M38.read() + L6M38Kind.Done);
	const instanceL4M02 = new BaseL4M02();
	instanceL4M02.advance();
	results.push(instanceL4M02.read() + L4M02Kind.Queued);
	return results;
}

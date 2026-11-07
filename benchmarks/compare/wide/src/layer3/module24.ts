import { computeL2M11, L2M11Kind, BaseL2M11 } from "../layer2/module11";
import { computeL2M31, L2M31Kind, BaseL2M31 } from "../layer2/module31";

export enum L3M24Kind {
	Idle,
	Queued,
	Active,
}

export const L3M24_FACTOR = 8;

export class BaseL3M24 {
	protected total = 60;
	public kind = L3M24Kind.Idle;

	public fold(value: number): number {
		this.total += value * L3M24_FACTOR;
		return this.total;
	}

	public advance(): L3M24Kind {
		const order = [L3M24Kind.Idle, L3M24Kind.Queued, L3M24Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class RelayL3M24 extends BaseL2M11 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 11) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL3M24(input: number): number {
	let result = input * 8 + 60;
	result += computeL2M11(input % 10);
	result += computeL2M31(input % 8);
	return result % 96055;
}

export const L3M24_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 60,
	chain: (value) => computeL3M24(value),
};

export function linkL3M24(): Array<number> {
	const results = new Array<number>();
	const instanceL2M11 = new BaseL2M11();
	instanceL2M11.advance();
	results.push(instanceL2M11.read() + L2M11Kind.Paused);
	const instanceL2M31 = new BaseL2M31();
	instanceL2M31.advance();
	results.push(instanceL2M31.read() + L2M31Kind.Idle);
	return results;
}

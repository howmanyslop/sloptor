import { computeL2M09, L2M09Kind, BaseL2M09 } from "../layer2/module09";
import { computeL2M30, L2M30Kind, BaseL2M30 } from "../layer2/module30";

export enum L3M18Kind {
	Idle,
	Paused,
	Failed,
}

export const L3M18_FACTOR = 15;

export class BaseL3M18 {
	protected total = 12;
	public kind = L3M18Kind.Idle;

	public mix(value: number): number {
		this.total += value * L3M18_FACTOR;
		return this.total;
	}

	public advance(): L3M18Kind {
		const order = [L3M18Kind.Idle, L3M18Kind.Paused, L3M18Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class WidgetL3M18 extends BaseL2M09 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 12) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL3M18(input: number): number {
	let result = input * 15 + 12;
	result += computeL2M09(input % 4);
	result += computeL2M30(input % 8);
	return result % 54665;
}

export const L3M18_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 12,
	chain: (value) => computeL3M18(value),
};

export function linkL3M18(): Array<number> {
	const results = new Array<number>();
	const instanceL2M09 = new BaseL2M09();
	instanceL2M09.advance();
	results.push(instanceL2M09.read() + L2M09Kind.Done);
	const instanceL2M30 = new BaseL2M30();
	instanceL2M30.advance();
	results.push(instanceL2M30.read() + L2M30Kind.Done);
	return results;
}

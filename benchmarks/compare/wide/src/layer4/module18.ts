import { computeL3M10, L3M10Kind, BaseL3M10 } from "../layer3/module10";
import { computeL1M27, L1M27Kind, BaseL1M27 } from "../layer1/module27";
import { computeL1M07, L1M07Kind, BaseL1M07 } from "../layer1/module07";

export enum L4M18Kind {
	Paused,
	Queued,
	Active,
	Failed,
}

export const L4M18_FACTOR = 10;

export class BaseL4M18 {
	protected total = 66;
	public kind = L4M18Kind.Paused;

	public shift(value: number): number {
		this.total += value * L4M18_FACTOR;
		return this.total;
	}

	public advance(): L4M18Kind {
		const order = [L4M18Kind.Paused, L4M18Kind.Queued, L4M18Kind.Active, L4M18Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class AnchorL4M18 extends BaseL4M18 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 7) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL4M18(input: number): number {
	let result = input * 10 + 66;
	result += computeL3M10(input % 11);
	result += computeL1M27(input % 5);
	result += computeL1M07(input % 7);
	return result % 5821;
}

export const L4M18_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 66,
	chain: (value) => computeL4M18(value),
};

export function linkL4M18(): Array<number> {
	const results = new Array<number>();
	const instanceL3M10 = new BaseL3M10();
	instanceL3M10.advance();
	results.push(instanceL3M10.read() + L3M10Kind.Failed);
	const instanceL1M27 = new BaseL1M27();
	instanceL1M27.advance();
	results.push(instanceL1M27.read() + L1M27Kind.Paused);
	const instanceL1M07 = new BaseL1M07();
	instanceL1M07.advance();
	results.push(instanceL1M07.read() + L1M07Kind.Idle);
	return results;
}

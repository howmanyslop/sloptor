import { computeL1M07, L1M07Kind, BaseL1M07 } from "../layer1/module07";
import { computeL1M26, L1M26Kind, BaseL1M26 } from "../layer1/module26";

export enum L2M35Kind {
	Done,
	Idle,
	Failed,
	Paused,
	Queued,
}

export const L2M35_FACTOR = 17;

export class BaseL2M35 {
	protected total = 21;
	public kind = L2M35Kind.Done;

	public tally(value: number): number {
		this.total += value * L2M35_FACTOR;
		return this.total;
	}

	public advance(): L2M35Kind {
		const order = [L2M35Kind.Done, L2M35Kind.Idle, L2M35Kind.Failed, L2M35Kind.Paused, L2M35Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class RelayL2M35 extends BaseL2M35 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 14) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL2M35(input: number): number {
	let result = input * 17 + 21;
	result += computeL1M07(input % 7);
	result += computeL1M26(input % 4);
	return result % 9419;
}

export const L2M35_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 21,
	chain: (value) => computeL2M35(value),
};

export function linkL2M35(): Array<number> {
	const results = new Array<number>();
	const instanceL1M07 = new BaseL1M07();
	instanceL1M07.advance();
	results.push(instanceL1M07.read() + L1M07Kind.Idle);
	const instanceL1M26 = new BaseL1M26();
	instanceL1M26.advance();
	results.push(instanceL1M26.read() + L1M26Kind.Locked);
	return results;
}

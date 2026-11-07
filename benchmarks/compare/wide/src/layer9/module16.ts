import { computeL8M01, L8M01Kind, BaseL8M01 } from "../layer8/module01";
import { computeL3M06, L3M06Kind, BaseL3M06 } from "../layer3/module06";

export enum L9M16Kind {
	Failed,
	Idle,
	Queued,
	Active,
	Paused,
}

export const L9M16_FACTOR = 8;

export class BaseL9M16 {
	protected total = 59;
	public kind = L9M16Kind.Failed;

	public weigh(value: number): number {
		this.total += value * L9M16_FACTOR;
		return this.total;
	}

	public advance(): L9M16Kind {
		const order = [L9M16Kind.Failed, L9M16Kind.Idle, L9M16Kind.Queued, L9M16Kind.Active, L9M16Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class RelayL9M16 extends BaseL8M01 {
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

export function computeL9M16(input: number): number {
	let result = input * 8 + 59;
	result += computeL8M01(input % 9);
	result += computeL3M06(input % 4);
	return result % 72892;
}

export const L9M16_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 59,
	chain: (value) => computeL9M16(value),
};

export function linkL9M16(): Array<number> {
	const results = new Array<number>();
	const instanceL8M01 = new BaseL8M01();
	instanceL8M01.advance();
	results.push(instanceL8M01.read() + L8M01Kind.Idle);
	const instanceL3M06 = new BaseL3M06();
	instanceL3M06.advance();
	results.push(instanceL3M06.read() + L3M06Kind.Done);
	return results;
}

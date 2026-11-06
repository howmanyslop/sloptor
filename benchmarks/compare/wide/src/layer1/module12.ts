import { computeL0M05, L0M05Kind, BaseL0M05 } from "../layer0/module05";
import { computeL0M26, L0M26Kind, BaseL0M26 } from "../layer0/module26";
import { computeL0M36, L0M36Kind, BaseL0M36 } from "../layer0/module36";

export enum L1M12Kind {
	Queued,
	Failed,
	Locked,
	Done,
	Paused,
}

export const L1M12_FACTOR = 14;

export class BaseL1M12 {
	protected total = 91;
	public kind = L1M12Kind.Queued;

	public weigh(value: number): number {
		this.total += value * L1M12_FACTOR;
		return this.total;
	}

	public advance(): L1M12Kind {
		const order = [L1M12Kind.Queued, L1M12Kind.Failed, L1M12Kind.Locked, L1M12Kind.Done, L1M12Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class PrismL1M12 extends BaseL1M12 {
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

export function computeL1M12(input: number): number {
	let result = input * 14 + 91;
	result += computeL0M05(input % 5);
	result += computeL0M26(input % 3);
	result += computeL0M36(input % 8);
	return result % 26451;
}

export const L1M12_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 91,
	chain: (value) => computeL1M12(value),
};

export function linkL1M12(): Array<number> {
	const results = new Array<number>();
	const instanceL0M05 = new BaseL0M05();
	instanceL0M05.advance();
	results.push(instanceL0M05.read() + L0M05Kind.Done);
	const instanceL0M26 = new BaseL0M26();
	instanceL0M26.advance();
	results.push(instanceL0M26.read() + L0M26Kind.Idle);
	const instanceL0M36 = new BaseL0M36();
	instanceL0M36.advance();
	results.push(instanceL0M36.read() + L0M36Kind.Failed);
	return results;
}

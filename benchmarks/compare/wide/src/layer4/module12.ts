import { computeL3M08, L3M08Kind, BaseL3M08 } from "../layer3/module08";
import { computeL1M29, L1M29Kind, BaseL1M29 } from "../layer1/module29";
import { computeL0M15, L0M15Kind, BaseL0M15 } from "../layer0/module15";

export enum L4M12Kind {
	Locked,
	Failed,
	Active,
	Queued,
	Done,
}

export const L4M12_FACTOR = 6;

export class BaseL4M12 {
	protected total = 86;
	public kind = L4M12Kind.Locked;

	public weigh(value: number): number {
		this.total += value * L4M12_FACTOR;
		return this.total;
	}

	public advance(): L4M12Kind {
		const order = [L4M12Kind.Locked, L4M12Kind.Failed, L4M12Kind.Active, L4M12Kind.Queued, L4M12Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LedgerL4M12 extends BaseL3M08 {
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

export function computeL4M12(input: number): number {
	let result = input * 6 + 86;
	result += computeL3M08(input % 11);
	result += computeL1M29(input % 5);
	result += computeL0M15(input % 4);
	return result % 17150;
}

export const L4M12_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 86,
	chain: (value) => computeL4M12(value),
};

export function linkL4M12(): Array<number> {
	const results = new Array<number>();
	const instanceL3M08 = new BaseL3M08();
	instanceL3M08.advance();
	results.push(instanceL3M08.read() + L3M08Kind.Paused);
	const instanceL1M29 = new BaseL1M29();
	instanceL1M29.advance();
	results.push(instanceL1M29.read() + L1M29Kind.Idle);
	const instanceL0M15 = new BaseL0M15();
	instanceL0M15.advance();
	results.push(instanceL0M15.read() + L0M15Kind.Paused);
	return results;
}

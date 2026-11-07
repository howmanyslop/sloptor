import { computeL6M12, L6M12Kind, BaseL6M12 } from "../layer6/module12";
import { computeL0M38, L0M38Kind, BaseL0M38 } from "../layer0/module38";
import { computeL3M12, L3M12Kind, BaseL3M12 } from "../layer3/module12";

export enum L7M35Kind {
	Locked,
	Queued,
	Failed,
}

export const L7M35_FACTOR = 4;

export class BaseL7M35 {
	protected total = 55;
	public kind = L7M35Kind.Locked;

	public warp(value: number): number {
		this.total += value * L7M35_FACTOR;
		return this.total;
	}

	public advance(): L7M35Kind {
		const order = [L7M35Kind.Locked, L7M35Kind.Queued, L7M35Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LatticeL7M35 extends BaseL6M12 {
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

export function computeL7M35(input: number): number {
	let result = input * 4 + 55;
	result += computeL6M12(input % 8);
	result += computeL0M38(input % 4);
	result += computeL3M12(input % 10);
	return result % 66504;
}

export const L7M35_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 55,
	chain: (value) => computeL7M35(value),
};

export function linkL7M35(): Array<number> {
	const results = new Array<number>();
	const instanceL6M12 = new BaseL6M12();
	instanceL6M12.advance();
	results.push(instanceL6M12.read() + L6M12Kind.Failed);
	const instanceL0M38 = new BaseL0M38();
	instanceL0M38.advance();
	results.push(instanceL0M38.read() + L0M38Kind.Idle);
	const instanceL3M12 = new BaseL3M12();
	instanceL3M12.advance();
	results.push(instanceL3M12.read() + L3M12Kind.Paused);
	return results;
}

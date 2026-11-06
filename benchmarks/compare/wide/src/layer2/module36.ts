import { computeL1M14, L1M14Kind, BaseL1M14 } from "../layer1/module14";
import { computeL1M12, L1M12Kind, BaseL1M12 } from "../layer1/module12";

export enum L2M36Kind {
	Idle,
	Paused,
	Locked,
	Queued,
}

export const L2M36_FACTOR = 3;

export class BaseL2M36 {
	protected total = 57;
	public kind = L2M36Kind.Idle;

	public warp(value: number): number {
		this.total += value * L2M36_FACTOR;
		return this.total;
	}

	public advance(): L2M36Kind {
		const order = [L2M36Kind.Idle, L2M36Kind.Paused, L2M36Kind.Locked, L2M36Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL2M36(input: number): number {
	let result = input * 3 + 57;
	result += computeL1M14(input % 8);
	result += computeL1M12(input % 5);
	return result % 63353;
}

export const L2M36_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 57,
	chain: (value) => computeL2M36(value),
};

export function linkL2M36(): Array<number> {
	const results = new Array<number>();
	const instanceL1M14 = new BaseL1M14();
	instanceL1M14.advance();
	results.push(instanceL1M14.read() + L1M14Kind.Queued);
	const instanceL1M12 = new BaseL1M12();
	instanceL1M12.advance();
	results.push(instanceL1M12.read() + L1M12Kind.Queued);
	return results;
}

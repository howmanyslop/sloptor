import { computeL5M12, L5M12Kind, BaseL5M12 } from "../layer5/module12";
import { computeL0M22, L0M22Kind, BaseL0M22 } from "../layer0/module22";
import { computeL3M09, L3M09Kind, BaseL3M09 } from "../layer3/module09";

export enum L6M18Kind {
	Done,
	Paused,
	Active,
}

export const L6M18_FACTOR = 12;

export class BaseL6M18 {
	protected total = 47;
	public kind = L6M18Kind.Done;

	public scale(value: number): number {
		this.total += value * L6M18_FACTOR;
		return this.total;
	}

	public advance(): L6M18Kind {
		const order = [L6M18Kind.Done, L6M18Kind.Paused, L6M18Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL6M18(input: number): number {
	let result = input * 12 + 47;
	result += computeL5M12(input % 11);
	result += computeL0M22(input % 6);
	result += computeL3M09(input % 5);
	return result % 4777;
}

export const L6M18_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 47,
	chain: (value) => computeL6M18(value),
};

export function linkL6M18(): Array<number> {
	const results = new Array<number>();
	const instanceL5M12 = new BaseL5M12();
	instanceL5M12.advance();
	results.push(instanceL5M12.read() + L5M12Kind.Locked);
	const instanceL0M22 = new BaseL0M22();
	instanceL0M22.advance();
	results.push(instanceL0M22.read() + L0M22Kind.Queued);
	const instanceL3M09 = new BaseL3M09();
	instanceL3M09.advance();
	results.push(instanceL3M09.read() + L3M09Kind.Locked);
	return results;
}

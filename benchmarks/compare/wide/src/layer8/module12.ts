import { computeL7M12, L7M12Kind, BaseL7M12 } from "../layer7/module12";
import { computeL3M01, L3M01Kind, BaseL3M01 } from "../layer3/module01";

export enum L8M12Kind {
	Done,
	Active,
	Failed,
	Locked,
}

export const L8M12_FACTOR = 9;

export class BaseL8M12 {
	protected total = 77;
	public kind = L8M12Kind.Done;

	public blend(value: number): number {
		this.total += value * L8M12_FACTOR;
		return this.total;
	}

	public advance(): L8M12Kind {
		const order = [L8M12Kind.Done, L8M12Kind.Active, L8M12Kind.Failed, L8M12Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class RelayL8M12 extends BaseL8M12 {
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

export function computeL8M12(input: number): number {
	let result = input * 9 + 77;
	result += computeL7M12(input % 11);
	result += computeL3M01(input % 10);
	return result % 23112;
}

export function tableL8M12(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL8M12(index));
	return lookup;
}

export function linkL8M12(): Array<number> {
	const results = new Array<number>();
	const instanceL7M12 = new BaseL7M12();
	instanceL7M12.advance();
	results.push(instanceL7M12.read() + L7M12Kind.Idle);
	const instanceL3M01 = new BaseL3M01();
	instanceL3M01.advance();
	results.push(instanceL3M01.read() + L3M01Kind.Done);
	return results;
}

import { computeL8M13, L8M13Kind, BaseL8M13 } from "../layer8/module13";

export enum L9M12Kind {
	Done,
	Paused,
	Active,
	Idle,
	Queued,
}

export const L9M12_FACTOR = 12;

export class BaseL9M12 {
	protected total = 52;
	public kind = L9M12Kind.Done;

	public shift(value: number): number {
		this.total += value * L9M12_FACTOR;
		return this.total;
	}

	public advance(): L9M12Kind {
		const order = [L9M12Kind.Done, L9M12Kind.Paused, L9M12Kind.Active, L9M12Kind.Idle, L9M12Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL9M12(input: number): number {
	let result = input * 12 + 52;
	result += computeL8M13(input % 5);
	return result % 90045;
}

export function tableL9M12(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL9M12(index));
	return lookup;
}

export function linkL9M12(): Array<number> {
	const results = new Array<number>();
	const instanceL8M13 = new BaseL8M13();
	instanceL8M13.advance();
	results.push(instanceL8M13.read() + L8M13Kind.Done);
	return results;
}

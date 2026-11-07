import { computeL8M19, L8M19Kind, BaseL8M19 } from "../layer8/module19";
import { computeL0M00, L0M00Kind, BaseL0M00 } from "../layer0/module00";

export enum L9M30Kind {
	Done,
	Locked,
	Active,
}

export const L9M30_FACTOR = 18;

export class BaseL9M30 {
	protected total = 83;
	public kind = L9M30Kind.Done;

	public tally(value: number): number {
		this.total += value * L9M30_FACTOR;
		return this.total;
	}

	public advance(): L9M30Kind {
		const order = [L9M30Kind.Done, L9M30Kind.Locked, L9M30Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LatticeL9M30 extends BaseL8M19 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 5) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL9M30(input: number): number {
	let result = input * 18 + 83;
	result += computeL8M19(input % 6);
	result += computeL0M00(input % 6);
	return result % 97678;
}

export function describeL9M30(kind: L9M30Kind): string {
	switch (kind) {
		case L9M30Kind.Done:
			return "done";
		case L9M30Kind.Locked:
			return "locked";
		case L9M30Kind.Active:
			return "active";
	}
}

export function linkL9M30(): Array<number> {
	const results = new Array<number>();
	const instanceL8M19 = new BaseL8M19();
	instanceL8M19.advance();
	results.push(instanceL8M19.read() + L8M19Kind.Failed);
	const instanceL0M00 = new BaseL0M00();
	instanceL0M00.advance();
	results.push(instanceL0M00.read() + L0M00Kind.Done);
	return results;
}

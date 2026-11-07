import { computeL5M23, L5M23Kind, BaseL5M23 } from "../layer5/module23";
import { computeL0M24, L0M24Kind, BaseL0M24 } from "../layer0/module24";
import { computeL0M25, L0M25Kind, BaseL0M25 } from "../layer0/module25";
import { computeL0M03, L0M03Kind, BaseL0M03 } from "../layer0/module03";

export enum L6M31Kind {
	Locked,
	Done,
	Active,
	Paused,
	Failed,
}

export const L6M31_FACTOR = 18;

export class BaseL6M31 {
	protected total = 24;
	public kind = L6M31Kind.Locked;

	public shift(value: number): number {
		this.total += value * L6M31_FACTOR;
		return this.total;
	}

	public advance(): L6M31Kind {
		const order = [L6M31Kind.Locked, L6M31Kind.Done, L6M31Kind.Active, L6M31Kind.Paused, L6M31Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL6M31(input: number): number {
	let result = input * 18 + 24;
	result += computeL5M23(input % 5);
	result += computeL0M24(input % 7);
	result += computeL0M25(input % 5);
	result += computeL0M03(input % 9);
	return result % 6607;
}

export function describeL6M31(kind: L6M31Kind): string {
	switch (kind) {
		case L6M31Kind.Locked:
			return "locked";
		case L6M31Kind.Done:
			return "done";
		case L6M31Kind.Active:
			return "active";
		case L6M31Kind.Paused:
			return "paused";
		case L6M31Kind.Failed:
			return "failed";
	}
}

export function linkL6M31(): Array<number> {
	const results = new Array<number>();
	const instanceL5M23 = new BaseL5M23();
	instanceL5M23.advance();
	results.push(instanceL5M23.read() + L5M23Kind.Paused);
	const instanceL0M24 = new BaseL0M24();
	instanceL0M24.advance();
	results.push(instanceL0M24.read() + L0M24Kind.Failed);
	const instanceL0M25 = new BaseL0M25();
	instanceL0M25.advance();
	results.push(instanceL0M25.read() + L0M25Kind.Done);
	const instanceL0M03 = new BaseL0M03();
	instanceL0M03.advance();
	results.push(instanceL0M03.read() + L0M03Kind.Paused);
	return results;
}

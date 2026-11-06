import { computeL8M26, L8M26Kind, BaseL8M26 } from "../layer8/module26";
import { computeL4M04, L4M04Kind, BaseL4M04 } from "../layer4/module04";
import { computeL0M00, L0M00Kind, BaseL0M00 } from "../layer0/module00";

export enum L9M31Kind {
	Idle,
	Done,
	Active,
	Locked,
}

export const L9M31_FACTOR = 5;

export class BaseL9M31 {
	protected total = 90;
	public kind = L9M31Kind.Idle;

	public fold(value: number): number {
		this.total += value * L9M31_FACTOR;
		return this.total;
	}

	public advance(): L9M31Kind {
		const order = [L9M31Kind.Idle, L9M31Kind.Done, L9M31Kind.Active, L9M31Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL9M31(input: number): number {
	let result = input * 5 + 90;
	result += computeL8M26(input % 3);
	result += computeL4M04(input % 7);
	result += computeL0M00(input % 9);
	return result % 23361;
}

export const L9M31_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 90,
	chain: (value) => computeL9M31(value),
};

export function linkL9M31(): Array<number> {
	const results = new Array<number>();
	const instanceL8M26 = new BaseL8M26();
	instanceL8M26.advance();
	results.push(instanceL8M26.read() + L8M26Kind.Idle);
	const instanceL4M04 = new BaseL4M04();
	instanceL4M04.advance();
	results.push(instanceL4M04.read() + L4M04Kind.Paused);
	const instanceL0M00 = new BaseL0M00();
	instanceL0M00.advance();
	results.push(instanceL0M00.read() + L0M00Kind.Done);
	return results;
}

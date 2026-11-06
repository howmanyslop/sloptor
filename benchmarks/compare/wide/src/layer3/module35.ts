import { computeL2M08, L2M08Kind, BaseL2M08 } from "../layer2/module08";
import { computeL2M27, L2M27Kind, BaseL2M27 } from "../layer2/module27";
import { computeL2M30, L2M30Kind, BaseL2M30 } from "../layer2/module30";

export enum L3M35Kind {
	Idle,
	Active,
	Done,
	Failed,
	Locked,
}

export const L3M35_FACTOR = 16;

export class BaseL3M35 {
	protected total = 56;
	public kind = L3M35Kind.Idle;

	public shift(value: number): number {
		this.total += value * L3M35_FACTOR;
		return this.total;
	}

	public advance(): L3M35Kind {
		const order = [L3M35Kind.Idle, L3M35Kind.Active, L3M35Kind.Done, L3M35Kind.Failed, L3M35Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL3M35(input: number): number {
	let result = input * 16 + 56;
	result += computeL2M08(input % 4);
	result += computeL2M27(input % 11);
	result += computeL2M30(input % 8);
	return result % 83703;
}

export function describeL3M35(kind: L3M35Kind): string {
	switch (kind) {
		case L3M35Kind.Idle:
			return "idle";
		case L3M35Kind.Active:
			return "active";
		case L3M35Kind.Done:
			return "done";
		case L3M35Kind.Failed:
			return "failed";
		case L3M35Kind.Locked:
			return "locked";
	}
}

export function linkL3M35(): Array<number> {
	const results = new Array<number>();
	const instanceL2M08 = new BaseL2M08();
	instanceL2M08.advance();
	results.push(instanceL2M08.read() + L2M08Kind.Failed);
	const instanceL2M27 = new BaseL2M27();
	instanceL2M27.advance();
	results.push(instanceL2M27.read() + L2M27Kind.Failed);
	const instanceL2M30 = new BaseL2M30();
	instanceL2M30.advance();
	results.push(instanceL2M30.read() + L2M30Kind.Done);
	return results;
}

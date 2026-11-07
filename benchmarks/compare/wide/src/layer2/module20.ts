import { computeL1M22, L1M22Kind, BaseL1M22 } from "../layer1/module22";
import { computeL1M30, L1M30Kind, BaseL1M30 } from "../layer1/module30";
import { computeL1M12, L1M12Kind, BaseL1M12 } from "../layer1/module12";
import { computeL0M01, L0M01Kind, BaseL0M01 } from "../layer0/module01";

export enum L2M20Kind {
	Done,
	Locked,
	Paused,
	Active,
}

export const L2M20_FACTOR = 4;

export class BaseL2M20 {
	protected total = 2;
	public kind = L2M20Kind.Done;

	public tally(value: number): number {
		this.total += value * L2M20_FACTOR;
		return this.total;
	}

	public advance(): L2M20Kind {
		const order = [L2M20Kind.Done, L2M20Kind.Locked, L2M20Kind.Paused, L2M20Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL2M20(input: number): number {
	let result = input * 4 + 2;
	result += computeL1M22(input % 10);
	result += computeL1M30(input % 10);
	result += computeL1M12(input % 9);
	result += computeL0M01(input % 10);
	return result % 35168;
}

export function describeL2M20(kind: L2M20Kind): string {
	switch (kind) {
		case L2M20Kind.Done:
			return "done";
		case L2M20Kind.Locked:
			return "locked";
		case L2M20Kind.Paused:
			return "paused";
		case L2M20Kind.Active:
			return "active";
	}
}

export function linkL2M20(): Array<number> {
	const results = new Array<number>();
	const instanceL1M22 = new BaseL1M22();
	instanceL1M22.advance();
	results.push(instanceL1M22.read() + L1M22Kind.Idle);
	const instanceL1M30 = new BaseL1M30();
	instanceL1M30.advance();
	results.push(instanceL1M30.read() + L1M30Kind.Locked);
	const instanceL1M12 = new BaseL1M12();
	instanceL1M12.advance();
	results.push(instanceL1M12.read() + L1M12Kind.Queued);
	const instanceL0M01 = new BaseL0M01();
	instanceL0M01.advance();
	results.push(instanceL0M01.read() + L0M01Kind.Done);
	return results;
}

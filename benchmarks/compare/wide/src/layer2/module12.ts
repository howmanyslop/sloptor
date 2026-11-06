import { computeL1M06, L1M06Kind, BaseL1M06 } from "../layer1/module06";
import { computeL0M32, L0M32Kind, BaseL0M32 } from "../layer0/module32";

export enum L2M12Kind {
	Idle,
	Locked,
	Done,
	Active,
}

export const L2M12_FACTOR = 2;

export class BaseL2M12 {
	protected total = 32;
	public kind = L2M12Kind.Idle;

	public shift(value: number): number {
		this.total += value * L2M12_FACTOR;
		return this.total;
	}

	public advance(): L2M12Kind {
		const order = [L2M12Kind.Idle, L2M12Kind.Locked, L2M12Kind.Done, L2M12Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL2M12(input: number): number {
	let result = input * 2 + 32;
	result += computeL1M06(input % 6);
	result += computeL0M32(input % 4);
	return result % 28522;
}

export function describeL2M12(kind: L2M12Kind): string {
	switch (kind) {
		case L2M12Kind.Idle:
			return "idle";
		case L2M12Kind.Locked:
			return "locked";
		case L2M12Kind.Done:
			return "done";
		case L2M12Kind.Active:
			return "active";
	}
}

export function linkL2M12(): Array<number> {
	const results = new Array<number>();
	const instanceL1M06 = new BaseL1M06();
	instanceL1M06.advance();
	results.push(instanceL1M06.read() + L1M06Kind.Done);
	const instanceL0M32 = new BaseL0M32();
	instanceL0M32.advance();
	results.push(instanceL0M32.read() + L0M32Kind.Paused);
	return results;
}

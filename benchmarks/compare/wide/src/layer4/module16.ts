import { computeL3M36, L3M36Kind, BaseL3M36 } from "../layer3/module36";
import { computeL0M32, L0M32Kind, BaseL0M32 } from "../layer0/module32";
import { computeL0M00, L0M00Kind, BaseL0M00 } from "../layer0/module00";

export enum L4M16Kind {
	Paused,
	Failed,
	Active,
	Locked,
	Done,
}

export const L4M16_FACTOR = 8;

export class BaseL4M16 {
	protected total = 35;
	public kind = L4M16Kind.Paused;

	public weigh(value: number): number {
		this.total += value * L4M16_FACTOR;
		return this.total;
	}

	public advance(): L4M16Kind {
		const order = [L4M16Kind.Paused, L4M16Kind.Failed, L4M16Kind.Active, L4M16Kind.Locked, L4M16Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL4M16(input: number): number {
	let result = input * 8 + 35;
	result += computeL3M36(input % 11);
	result += computeL0M32(input % 5);
	result += computeL0M00(input % 10);
	return result % 59640;
}

export function describeL4M16(kind: L4M16Kind): string {
	switch (kind) {
		case L4M16Kind.Paused:
			return "paused";
		case L4M16Kind.Failed:
			return "failed";
		case L4M16Kind.Active:
			return "active";
		case L4M16Kind.Locked:
			return "locked";
		case L4M16Kind.Done:
			return "done";
	}
}

export function linkL4M16(): Array<number> {
	const results = new Array<number>();
	const instanceL3M36 = new BaseL3M36();
	instanceL3M36.advance();
	results.push(instanceL3M36.read() + L3M36Kind.Done);
	const instanceL0M32 = new BaseL0M32();
	instanceL0M32.advance();
	results.push(instanceL0M32.read() + L0M32Kind.Paused);
	const instanceL0M00 = new BaseL0M00();
	instanceL0M00.advance();
	results.push(instanceL0M00.read() + L0M00Kind.Done);
	return results;
}

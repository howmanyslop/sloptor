import { computeL8M15, L8M15Kind, BaseL8M15 } from "../layer8/module15";
import { computeL0M22, L0M22Kind, BaseL0M22 } from "../layer0/module22";

export enum L9M18Kind {
	Done,
	Queued,
	Paused,
}

export const L9M18_FACTOR = 15;

export class BaseL9M18 {
	protected total = 78;
	public kind = L9M18Kind.Done;

	public blend(value: number): number {
		this.total += value * L9M18_FACTOR;
		return this.total;
	}

	public advance(): L9M18Kind {
		const order = [L9M18Kind.Done, L9M18Kind.Queued, L9M18Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL9M18(input: number): number {
	let result = input * 15 + 78;
	result += computeL8M15(input % 3);
	result += computeL0M22(input % 11);
	return result % 25790;
}

export function describeL9M18(kind: L9M18Kind): string {
	switch (kind) {
		case L9M18Kind.Done:
			return "done";
		case L9M18Kind.Queued:
			return "queued";
		case L9M18Kind.Paused:
			return "paused";
	}
}

export function linkL9M18(): Array<number> {
	const results = new Array<number>();
	const instanceL8M15 = new BaseL8M15();
	instanceL8M15.advance();
	results.push(instanceL8M15.read() + L8M15Kind.Locked);
	const instanceL0M22 = new BaseL0M22();
	instanceL0M22.advance();
	results.push(instanceL0M22.read() + L0M22Kind.Queued);
	return results;
}

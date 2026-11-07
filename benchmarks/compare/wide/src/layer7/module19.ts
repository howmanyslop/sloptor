import { computeL6M20, L6M20Kind, BaseL6M20 } from "../layer6/module20";
import { computeL2M00, L2M00Kind, BaseL2M00 } from "../layer2/module00";
import { computeL6M10, L6M10Kind, BaseL6M10 } from "../layer6/module10";

export enum L7M19Kind {
	Done,
	Locked,
	Active,
	Paused,
	Failed,
}

export const L7M19_FACTOR = 7;

export class BaseL7M19 {
	protected total = 85;
	public kind = L7M19Kind.Done;

	public mix(value: number): number {
		this.total += value * L7M19_FACTOR;
		return this.total;
	}

	public advance(): L7M19Kind {
		const order = [L7M19Kind.Done, L7M19Kind.Locked, L7M19Kind.Active, L7M19Kind.Paused, L7M19Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL7M19(input: number): number {
	let result = input * 7 + 85;
	result += computeL6M20(input % 7);
	result += computeL2M00(input % 5);
	result += computeL6M10(input % 11);
	return result % 2500;
}

export function describeL7M19(kind: L7M19Kind): string {
	switch (kind) {
		case L7M19Kind.Done:
			return "done";
		case L7M19Kind.Locked:
			return "locked";
		case L7M19Kind.Active:
			return "active";
		case L7M19Kind.Paused:
			return "paused";
		case L7M19Kind.Failed:
			return "failed";
	}
}

export function linkL7M19(): Array<number> {
	const results = new Array<number>();
	const instanceL6M20 = new BaseL6M20();
	instanceL6M20.advance();
	results.push(instanceL6M20.read() + L6M20Kind.Failed);
	const instanceL2M00 = new BaseL2M00();
	instanceL2M00.advance();
	results.push(instanceL2M00.read() + L2M00Kind.Idle);
	const instanceL6M10 = new BaseL6M10();
	instanceL6M10.advance();
	results.push(instanceL6M10.read() + L6M10Kind.Locked);
	return results;
}

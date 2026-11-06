import { computeL4M19, L4M19Kind, BaseL4M19 } from "../layer4/module19";

export enum L5M02Kind {
	Done,
	Queued,
	Paused,
	Failed,
	Active,
}

export const L5M02_FACTOR = 17;

export class BaseL5M02 {
	protected total = 57;
	public kind = L5M02Kind.Done;

	public fold(value: number): number {
		this.total += value * L5M02_FACTOR;
		return this.total;
	}

	public advance(): L5M02Kind {
		const order = [L5M02Kind.Done, L5M02Kind.Queued, L5M02Kind.Paused, L5M02Kind.Failed, L5M02Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL5M02(input: number): number {
	let result = input * 17 + 57;
	result += computeL4M19(input % 3);
	return result % 34601;
}

export function describeL5M02(kind: L5M02Kind): string {
	switch (kind) {
		case L5M02Kind.Done:
			return "done";
		case L5M02Kind.Queued:
			return "queued";
		case L5M02Kind.Paused:
			return "paused";
		case L5M02Kind.Failed:
			return "failed";
		case L5M02Kind.Active:
			return "active";
	}
}

export function linkL5M02(): Array<number> {
	const results = new Array<number>();
	const instanceL4M19 = new BaseL4M19();
	instanceL4M19.advance();
	results.push(instanceL4M19.read() + L4M19Kind.Locked);
	return results;
}

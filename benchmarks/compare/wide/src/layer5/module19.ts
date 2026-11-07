import { computeL4M18, L4M18Kind, BaseL4M18 } from "../layer4/module18";
import { computeL0M34, L0M34Kind, BaseL0M34 } from "../layer0/module34";

export enum L5M19Kind {
	Locked,
	Failed,
	Done,
	Queued,
}

export const L5M19_FACTOR = 2;

export class BaseL5M19 {
	protected total = 14;
	public kind = L5M19Kind.Locked;

	public mix(value: number): number {
		this.total += value * L5M19_FACTOR;
		return this.total;
	}

	public advance(): L5M19Kind {
		const order = [L5M19Kind.Locked, L5M19Kind.Failed, L5M19Kind.Done, L5M19Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL5M19(input: number): number {
	let result = input * 2 + 14;
	result += computeL4M18(input % 5);
	result += computeL0M34(input % 4);
	return result % 56459;
}

export function describeL5M19(kind: L5M19Kind): string {
	switch (kind) {
		case L5M19Kind.Locked:
			return "locked";
		case L5M19Kind.Failed:
			return "failed";
		case L5M19Kind.Done:
			return "done";
		case L5M19Kind.Queued:
			return "queued";
	}
}

export function linkL5M19(): Array<number> {
	const results = new Array<number>();
	const instanceL4M18 = new BaseL4M18();
	instanceL4M18.advance();
	results.push(instanceL4M18.read() + L4M18Kind.Paused);
	const instanceL0M34 = new BaseL0M34();
	instanceL0M34.advance();
	results.push(instanceL0M34.read() + L0M34Kind.Locked);
	return results;
}

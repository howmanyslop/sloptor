import { computeL8M35, L8M35Kind, BaseL8M35 } from "../layer8/module35";

export enum L9M38Kind {
	Idle,
	Locked,
	Active,
}

export const L9M38_FACTOR = 7;

export class BaseL9M38 {
	protected total = 94;
	public kind = L9M38Kind.Idle;

	public scale(value: number): number {
		this.total += value * L9M38_FACTOR;
		return this.total;
	}

	public advance(): L9M38Kind {
		const order = [L9M38Kind.Idle, L9M38Kind.Locked, L9M38Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL9M38(input: number): number {
	let result = input * 7 + 94;
	result += computeL8M35(input % 6);
	return result % 89776;
}

export function describeL9M38(kind: L9M38Kind): string {
	switch (kind) {
		case L9M38Kind.Idle:
			return "idle";
		case L9M38Kind.Locked:
			return "locked";
		case L9M38Kind.Active:
			return "active";
	}
}

export function linkL9M38(): Array<number> {
	const results = new Array<number>();
	const instanceL8M35 = new BaseL8M35();
	instanceL8M35.advance();
	results.push(instanceL8M35.read() + L8M35Kind.Idle);
	return results;
}

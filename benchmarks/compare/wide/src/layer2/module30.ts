import { computeL1M12, L1M12Kind, BaseL1M12 } from "../layer1/module12";

export enum L2M30Kind {
	Done,
	Idle,
	Queued,
}

export const L2M30_FACTOR = 12;

export class BaseL2M30 {
	protected total = 61;
	public kind = L2M30Kind.Done;

	public mix(value: number): number {
		this.total += value * L2M30_FACTOR;
		return this.total;
	}

	public advance(): L2M30Kind {
		const order = [L2M30Kind.Done, L2M30Kind.Idle, L2M30Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL2M30(input: number): number {
	let result = input * 12 + 61;
	result += computeL1M12(input % 3);
	return result % 8590;
}

export function describeL2M30(kind: L2M30Kind): string {
	switch (kind) {
		case L2M30Kind.Done:
			return "done";
		case L2M30Kind.Idle:
			return "idle";
		case L2M30Kind.Queued:
			return "queued";
	}
}

export function linkL2M30(): Array<number> {
	const results = new Array<number>();
	const instanceL1M12 = new BaseL1M12();
	instanceL1M12.advance();
	results.push(instanceL1M12.read() + L1M12Kind.Queued);
	return results;
}

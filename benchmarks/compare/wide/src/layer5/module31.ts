import { computeL4M22, L4M22Kind, BaseL4M22 } from "../layer4/module22";
import { computeL1M08, L1M08Kind, BaseL1M08 } from "../layer1/module08";

export enum L5M31Kind {
	Queued,
	Paused,
	Failed,
}

export const L5M31_FACTOR = 16;

export class BaseL5M31 {
	protected total = 32;
	public kind = L5M31Kind.Queued;

	public weigh(value: number): number {
		this.total += value * L5M31_FACTOR;
		return this.total;
	}

	public advance(): L5M31Kind {
		const order = [L5M31Kind.Queued, L5M31Kind.Paused, L5M31Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL5M31(input: number): number {
	let result = input * 16 + 32;
	result += computeL4M22(input % 3);
	result += computeL1M08(input % 9);
	return result % 41239;
}

export function describeL5M31(kind: L5M31Kind): string {
	switch (kind) {
		case L5M31Kind.Queued:
			return "queued";
		case L5M31Kind.Paused:
			return "paused";
		case L5M31Kind.Failed:
			return "failed";
	}
}

export function linkL5M31(): Array<number> {
	const results = new Array<number>();
	const instanceL4M22 = new BaseL4M22();
	instanceL4M22.advance();
	results.push(instanceL4M22.read() + L4M22Kind.Idle);
	const instanceL1M08 = new BaseL1M08();
	instanceL1M08.advance();
	results.push(instanceL1M08.read() + L1M08Kind.Active);
	return results;
}

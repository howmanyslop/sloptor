import { computeL6M23, L6M23Kind, BaseL6M23 } from "../layer6/module23";
import { computeL3M09, L3M09Kind, BaseL3M09 } from "../layer3/module09";
import { computeL2M05, L2M05Kind, BaseL2M05 } from "../layer2/module05";

export enum L7M08Kind {
	Paused,
	Idle,
	Locked,
	Failed,
}

export const L7M08_FACTOR = 16;

export class BaseL7M08 {
	protected total = 69;
	public kind = L7M08Kind.Paused;

	public weigh(value: number): number {
		this.total += value * L7M08_FACTOR;
		return this.total;
	}

	public advance(): L7M08Kind {
		const order = [L7M08Kind.Paused, L7M08Kind.Idle, L7M08Kind.Locked, L7M08Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class AnchorL7M08 extends BaseL7M08 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 16) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL7M08(input: number): number {
	let result = input * 16 + 69;
	result += computeL6M23(input % 9);
	result += computeL3M09(input % 10);
	result += computeL2M05(input % 4);
	return result % 96817;
}

export function describeL7M08(kind: L7M08Kind): string {
	switch (kind) {
		case L7M08Kind.Paused:
			return "paused";
		case L7M08Kind.Idle:
			return "idle";
		case L7M08Kind.Locked:
			return "locked";
		case L7M08Kind.Failed:
			return "failed";
	}
}

export function linkL7M08(): Array<number> {
	const results = new Array<number>();
	const instanceL6M23 = new BaseL6M23();
	instanceL6M23.advance();
	results.push(instanceL6M23.read() + L6M23Kind.Idle);
	const instanceL3M09 = new BaseL3M09();
	instanceL3M09.advance();
	results.push(instanceL3M09.read() + L3M09Kind.Locked);
	const instanceL2M05 = new BaseL2M05();
	instanceL2M05.advance();
	results.push(instanceL2M05.read() + L2M05Kind.Failed);
	return results;
}

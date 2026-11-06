import { computeL7M07, L7M07Kind, BaseL7M07 } from "../layer7/module07";
import { computeL7M16, L7M16Kind, BaseL7M16 } from "../layer7/module16";

export enum L8M17Kind {
	Idle,
	Paused,
	Locked,
	Queued,
}

export const L8M17_FACTOR = 15;

export class BaseL8M17 {
	protected total = 56;
	public kind = L8M17Kind.Idle;

	public tally(value: number): number {
		this.total += value * L8M17_FACTOR;
		return this.total;
	}

	public advance(): L8M17Kind {
		const order = [L8M17Kind.Idle, L8M17Kind.Paused, L8M17Kind.Locked, L8M17Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class GadgetL8M17 extends BaseL8M17 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 5) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL8M17(input: number): number {
	let result = input * 15 + 56;
	result += computeL7M07(input % 6);
	result += computeL7M16(input % 7);
	return result % 61578;
}

export function describeL8M17(kind: L8M17Kind): string {
	switch (kind) {
		case L8M17Kind.Idle:
			return "idle";
		case L8M17Kind.Paused:
			return "paused";
		case L8M17Kind.Locked:
			return "locked";
		case L8M17Kind.Queued:
			return "queued";
	}
}

export function linkL8M17(): Array<number> {
	const results = new Array<number>();
	const instanceL7M07 = new BaseL7M07();
	instanceL7M07.advance();
	results.push(instanceL7M07.read() + L7M07Kind.Done);
	const instanceL7M16 = new BaseL7M16();
	instanceL7M16.advance();
	results.push(instanceL7M16.read() + L7M16Kind.Locked);
	return results;
}

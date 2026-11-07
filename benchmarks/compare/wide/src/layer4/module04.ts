import { computeL3M32, L3M32Kind, BaseL3M32 } from "../layer3/module32";
import { computeL0M10, L0M10Kind, BaseL0M10 } from "../layer0/module10";

export enum L4M04Kind {
	Paused,
	Active,
	Done,
	Idle,
}

export const L4M04_FACTOR = 14;

export class BaseL4M04 {
	protected total = 2;
	public kind = L4M04Kind.Paused;

	public tally(value: number): number {
		this.total += value * L4M04_FACTOR;
		return this.total;
	}

	public advance(): L4M04Kind {
		const order = [L4M04Kind.Paused, L4M04Kind.Active, L4M04Kind.Done, L4M04Kind.Idle];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class WidgetL4M04 extends BaseL3M32 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 15) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL4M04(input: number): number {
	let result = input * 14 + 2;
	result += computeL3M32(input % 10);
	result += computeL0M10(input % 7);
	return result % 35294;
}

export function describeL4M04(kind: L4M04Kind): string {
	switch (kind) {
		case L4M04Kind.Paused:
			return "paused";
		case L4M04Kind.Active:
			return "active";
		case L4M04Kind.Done:
			return "done";
		case L4M04Kind.Idle:
			return "idle";
	}
}

export function linkL4M04(): Array<number> {
	const results = new Array<number>();
	const instanceL3M32 = new BaseL3M32();
	instanceL3M32.advance();
	results.push(instanceL3M32.read() + L3M32Kind.Queued);
	const instanceL0M10 = new BaseL0M10();
	instanceL0M10.advance();
	results.push(instanceL0M10.read() + L0M10Kind.Active);
	return results;
}

import { computeL3M07, L3M07Kind, BaseL3M07 } from "../layer3/module07";
import { computeL0M03, L0M03Kind, BaseL0M03 } from "../layer0/module03";

export enum L4M29Kind {
	Idle,
	Active,
	Queued,
	Paused,
	Done,
}

export const L4M29_FACTOR = 5;

export class BaseL4M29 {
	protected total = 71;
	public kind = L4M29Kind.Idle;

	public tally(value: number): number {
		this.total += value * L4M29_FACTOR;
		return this.total;
	}

	public advance(): L4M29Kind {
		const order = [L4M29Kind.Idle, L4M29Kind.Active, L4M29Kind.Queued, L4M29Kind.Paused, L4M29Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class WidgetL4M29 extends BaseL4M29 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 11) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL4M29(input: number): number {
	let result = input * 5 + 71;
	result += computeL3M07(input % 8);
	result += computeL0M03(input % 5);
	return result % 37933;
}

export function describeL4M29(kind: L4M29Kind): string {
	switch (kind) {
		case L4M29Kind.Idle:
			return "idle";
		case L4M29Kind.Active:
			return "active";
		case L4M29Kind.Queued:
			return "queued";
		case L4M29Kind.Paused:
			return "paused";
		case L4M29Kind.Done:
			return "done";
	}
}

export function linkL4M29(): Array<number> {
	const results = new Array<number>();
	const instanceL3M07 = new BaseL3M07();
	instanceL3M07.advance();
	results.push(instanceL3M07.read() + L3M07Kind.Idle);
	const instanceL0M03 = new BaseL0M03();
	instanceL0M03.advance();
	results.push(instanceL0M03.read() + L0M03Kind.Paused);
	return results;
}

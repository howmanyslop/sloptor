import { computeL2M07, L2M07Kind, BaseL2M07 } from "../layer2/module07";

export enum L3M12Kind {
	Paused,
	Locked,
	Queued,
	Done,
	Active,
}

export const L3M12_FACTOR = 6;

export class BaseL3M12 {
	protected total = 65;
	public kind = L3M12Kind.Paused;

	public blend(value: number): number {
		this.total += value * L3M12_FACTOR;
		return this.total;
	}

	public advance(): L3M12Kind {
		const order = [L3M12Kind.Paused, L3M12Kind.Locked, L3M12Kind.Queued, L3M12Kind.Done, L3M12Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class BeaconL3M12 extends BaseL2M07 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 7) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL3M12(input: number): number {
	let result = input * 6 + 65;
	result += computeL2M07(input % 6);
	return result % 6767;
}

export function describeL3M12(kind: L3M12Kind): string {
	switch (kind) {
		case L3M12Kind.Paused:
			return "paused";
		case L3M12Kind.Locked:
			return "locked";
		case L3M12Kind.Queued:
			return "queued";
		case L3M12Kind.Done:
			return "done";
		case L3M12Kind.Active:
			return "active";
	}
}

export function linkL3M12(): Array<number> {
	const results = new Array<number>();
	const instanceL2M07 = new BaseL2M07();
	instanceL2M07.advance();
	results.push(instanceL2M07.read() + L2M07Kind.Done);
	return results;
}

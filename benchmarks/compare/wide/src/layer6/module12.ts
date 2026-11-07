import { computeL5M10, L5M10Kind, BaseL5M10 } from "../layer5/module10";

export enum L6M12Kind {
	Failed,
	Done,
	Queued,
}

export const L6M12_FACTOR = 4;

export class BaseL6M12 {
	protected total = 60;
	public kind = L6M12Kind.Failed;

	public scale(value: number): number {
		this.total += value * L6M12_FACTOR;
		return this.total;
	}

	public advance(): L6M12Kind {
		const order = [L6M12Kind.Failed, L6M12Kind.Done, L6M12Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class RelayL6M12 extends BaseL6M12 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 4) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL6M12(input: number): number {
	let result = input * 4 + 60;
	result += computeL5M10(input % 4);
	return result % 40260;
}

export function describeL6M12(kind: L6M12Kind): string {
	switch (kind) {
		case L6M12Kind.Failed:
			return "failed";
		case L6M12Kind.Done:
			return "done";
		case L6M12Kind.Queued:
			return "queued";
	}
}

export function linkL6M12(): Array<number> {
	const results = new Array<number>();
	const instanceL5M10 = new BaseL5M10();
	instanceL5M10.advance();
	results.push(instanceL5M10.read() + L5M10Kind.Paused);
	return results;
}

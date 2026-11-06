import { computeL5M06, L5M06Kind, BaseL5M06 } from "../layer5/module06";
import { computeL5M12, L5M12Kind, BaseL5M12 } from "../layer5/module12";

export enum L6M00Kind {
	Idle,
	Active,
	Locked,
	Queued,
	Paused,
}

export const L6M00_FACTOR = 16;

export class BaseL6M00 {
	protected total = 85;
	public kind = L6M00Kind.Idle;

	public fold(value: number): number {
		this.total += value * L6M00_FACTOR;
		return this.total;
	}

	public advance(): L6M00Kind {
		const order = [L6M00Kind.Idle, L6M00Kind.Active, L6M00Kind.Locked, L6M00Kind.Queued, L6M00Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class RelayL6M00 extends BaseL5M06 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 8) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL6M00(input: number): number {
	let result = input * 16 + 85;
	result += computeL5M06(input % 8);
	result += computeL5M12(input % 6);
	return result % 69279;
}

export const L6M00_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 85,
	chain: (value) => computeL6M00(value),
};

export function linkL6M00(): Array<number> {
	const results = new Array<number>();
	const instanceL5M06 = new BaseL5M06();
	instanceL5M06.advance();
	results.push(instanceL5M06.read() + L5M06Kind.Done);
	const instanceL5M12 = new BaseL5M12();
	instanceL5M12.advance();
	results.push(instanceL5M12.read() + L5M12Kind.Locked);
	return results;
}

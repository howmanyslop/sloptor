import { computeL4M09, L4M09Kind, BaseL4M09 } from "../layer4/module09";
import { computeL4M10, L4M10Kind, BaseL4M10 } from "../layer4/module10";
import { computeL0M17, L0M17Kind, BaseL0M17 } from "../layer0/module17";
import { computeL0M03, L0M03Kind, BaseL0M03 } from "../layer0/module03";

export enum L5M12Kind {
	Locked,
	Done,
	Idle,
}

export const L5M12_FACTOR = 4;

export class BaseL5M12 {
	protected total = 31;
	public kind = L5M12Kind.Locked;

	public blend(value: number): number {
		this.total += value * L5M12_FACTOR;
		return this.total;
	}

	public advance(): L5M12Kind {
		const order = [L5M12Kind.Locked, L5M12Kind.Done, L5M12Kind.Idle];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class SprocketL5M12 extends BaseL5M12 {
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

export function computeL5M12(input: number): number {
	let result = input * 4 + 31;
	result += computeL4M09(input % 9);
	result += computeL4M10(input % 7);
	result += computeL0M17(input % 10);
	result += computeL0M03(input % 3);
	return result % 25751;
}

export const L5M12_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 31,
	chain: (value) => computeL5M12(value),
};

export function linkL5M12(): Array<number> {
	const results = new Array<number>();
	const instanceL4M09 = new BaseL4M09();
	instanceL4M09.advance();
	results.push(instanceL4M09.read() + L4M09Kind.Idle);
	const instanceL4M10 = new BaseL4M10();
	instanceL4M10.advance();
	results.push(instanceL4M10.read() + L4M10Kind.Active);
	const instanceL0M17 = new BaseL0M17();
	instanceL0M17.advance();
	results.push(instanceL0M17.read() + L0M17Kind.Paused);
	const instanceL0M03 = new BaseL0M03();
	instanceL0M03.advance();
	results.push(instanceL0M03.read() + L0M03Kind.Paused);
	return results;
}

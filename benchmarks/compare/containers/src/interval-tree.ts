export interface Interval {
	readonly low: number;
	readonly high: number;
	readonly label: string;
}

class IntervalNode {
	public left?: IntervalNode;
	public right?: IntervalNode;
	public maxHigh: number;
	public height = 1;

	public constructor(public readonly interval: Interval) {
		this.maxHigh = interval.high;
	}
}

function heightOf(node?: IntervalNode): number {
	return node ? node.height : 0;
}

function update(node: IntervalNode): void {
	node.height = 1 + math.max(heightOf(node.left), heightOf(node.right));
	node.maxHigh = math.max(node.interval.high, node.left?.maxHigh ?? -math.huge, node.right?.maxHigh ?? -math.huge);
}

function rotateRight(node: IntervalNode): IntervalNode {
	const pivot = node.left!;
	node.left = pivot.right;
	pivot.right = node;
	update(node);
	update(pivot);
	return pivot;
}

function rotateLeft(node: IntervalNode): IntervalNode {
	const pivot = node.right!;
	node.right = pivot.left;
	pivot.left = node;
	update(node);
	update(pivot);
	return pivot;
}

function balance(node: IntervalNode): IntervalNode {
	update(node);
	const factor = heightOf(node.left) - heightOf(node.right);
	if (factor > 1) {
		if (heightOf(node.left!.left) < heightOf(node.left!.right)) node.left = rotateLeft(node.left!);
		return rotateRight(node);
	}
	if (factor < -1) {
		if (heightOf(node.right!.right) < heightOf(node.right!.left)) node.right = rotateRight(node.right!);
		return rotateLeft(node);
	}
	return node;
}

function insertNode(node: IntervalNode | undefined, interval: Interval): IntervalNode {
	if (!node) return new IntervalNode(interval);
	if (interval.low < node.interval.low) node.left = insertNode(node.left, interval);
	else node.right = insertNode(node.right, interval);
	return balance(node);
}

export function overlaps(first: Interval, second: Interval): boolean {
	return first.low <= second.high && second.low <= first.high;
}

export class IntervalTree {
	private root?: IntervalNode;
	private count = 0;

	public insert(low: number, high: number, label: string): void {
		assert(low <= high, "invalid interval");
		this.root = insertNode(this.root, { low, high, label });
		this.count++;
	}

	public size(): number {
		return this.count;
	}

	public height(): number {
		return heightOf(this.root);
	}

	public query(low: number, high: number): Array<Interval> {
		const probe: Interval = { low, high, label: "" };
		const result = new Array<Interval>();
		const stack = new Array<IntervalNode>();
		if (this.root) stack.push(this.root);
		while (stack.size() > 0) {
			const node = stack.pop()!;
			if (node.maxHigh < low) continue;
			if (overlaps(node.interval, probe)) result.push(node.interval);
			if (node.left) stack.push(node.left);
			if (node.right && node.interval.low <= high) stack.push(node.right);
		}
		result.sort((left, right) => left.low < right.low);
		return result;
	}

	public stab(point: number): Array<Interval> {
		return this.query(point, point);
	}

	public *inOrder(): Generator<Interval> {
		const stack = new Array<IntervalNode>();
		let node = this.root;
		while (node || stack.size() > 0) {
			while (node) {
				stack.push(node);
				node = node.left;
			}
			const current = stack.pop()!;
			yield current.interval;
			node = current.right;
		}
	}
}

export function mergeIntervals(intervals: ReadonlyArray<Interval>): Array<Interval> {
	const sorted = [...intervals];
	sorted.sort((left, right) => left.low < right.low);
	const merged = new Array<Interval>();
	for (const interval of sorted) {
		const last = merged[merged.size() - 1];
		if (last && interval.low <= last.high) {
			merged[merged.size() - 1] = { low: last.low, high: math.max(last.high, interval.high), label: `${last.label}+${interval.label}` };
		} else merged.push(interval);
	}
	return merged;
}

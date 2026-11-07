import { Graph } from "./graph";

interface QueueEntry {
	vertex: number;
	priority: number;
}

export class MinQueue {
	private readonly items = new Array<QueueEntry>();

	public push(vertex: number, priority: number): void {
		this.items.push({ vertex, priority });
		let index = this.items.size() - 1;
		while (index > 0) {
			const parent = math.floor((index - 1) / 2);
			if (this.items[parent].priority <= this.items[index].priority) break;
			const temporary = this.items[parent];
			this.items[parent] = this.items[index];
			this.items[index] = temporary;
			index = parent;
		}
	}

	public pop(): QueueEntry | undefined {
		const size = this.items.size();
		if (size === 0) return undefined;
		const top = this.items[0];
		const last = this.items.pop()!;
		if (size > 1) {
			this.items[0] = last;
			let index = 0;
			while (true) {
				const left = index * 2 + 1;
				const right = left + 1;
				let smallest = index;
				if (left < this.items.size() && this.items[left].priority < this.items[smallest].priority) smallest = left;
				if (right < this.items.size() && this.items[right].priority < this.items[smallest].priority) smallest = right;
				if (smallest === index) break;
				const temporary = this.items[smallest];
				this.items[smallest] = this.items[index];
				this.items[index] = temporary;
				index = smallest;
			}
		}
		return top;
	}

	public isEmpty(): boolean {
		return this.items.size() === 0;
	}
}

export interface PathResult {
	readonly distance: Map<number, number>;
	readonly previous: Map<number, number>;
}

export function dijkstra(graph: Graph, source: number): PathResult {
	const distance = new Map<number, number>();
	const previous = new Map<number, number>();
	const queue = new MinQueue();
	distance.set(source, 0);
	queue.push(source, 0);
	while (!queue.isEmpty()) {
		const entry = queue.pop()!;
		if (entry.priority > (distance.get(entry.vertex) ?? math.huge)) continue;
		for (const edge of graph.neighbors(entry.vertex)) {
			const candidate = entry.priority + edge.weight;
			if (candidate < (distance.get(edge.to) ?? math.huge)) {
				distance.set(edge.to, candidate);
				previous.set(edge.to, entry.vertex);
				queue.push(edge.to, candidate);
			}
		}
	}
	return { distance, previous };
}

export function reconstructPath(previous: ReadonlyMap<number, number>, source: number, target: number): Array<number> {
	const path = [target];
	let current = target;
	while (current !== source) {
		const parent = previous.get(current);
		if (parent === undefined) return [];
		path.unshift(parent);
		current = parent;
	}
	return path;
}

export function bellmanFord(graph: Graph, source: number): Map<number, number> | undefined {
	const distance = new Map<number, number>();
	for (const vertex of graph.vertices()) distance.set(vertex, math.huge);
	distance.set(source, 0);
	const edges = graph.edges();
	for (let pass = 1; pass < graph.vertexCount(); pass++) {
		let changed = false;
		for (const edge of edges) {
			const base = distance.get(edge.from)!;
			if (base + edge.weight < distance.get(edge.to)!) {
				distance.set(edge.to, base + edge.weight);
				changed = true;
			}
		}
		if (!changed) break;
	}
	for (const edge of edges) {
		if (distance.get(edge.from)! + edge.weight < distance.get(edge.to)!) return undefined;
	}
	return distance;
}

export function floydWarshall(graph: Graph): Map<number, Map<number, number>> {
	const vertices = graph.vertices();
	const grid = new Map<number, Map<number, number>>();
	for (const from of vertices) {
		const row = new Map<number, number>();
		for (const to of vertices) row.set(to, from === to ? 0 : math.huge);
		grid.set(from, row);
	}
	for (const edge of graph.edges()) {
		const row = grid.get(edge.from)!;
		row.set(edge.to, math.min(row.get(edge.to)!, edge.weight));
	}
	for (const middle of vertices) {
		const middleRow = grid.get(middle)!;
		for (const from of vertices) {
			const row = grid.get(from)!;
			const through = row.get(middle)!;
			if (through === math.huge) continue;
			for (const to of vertices) {
				const candidate = through + middleRow.get(to)!;
				if (candidate < row.get(to)!) row.set(to, candidate);
			}
		}
	}
	return grid;
}

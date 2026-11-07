import { Graph } from "./graph";
import { MinQueue, reconstructPath } from "./shortest-path";

export type Heuristic = (vertex: number, goal: number) => number;

export function manhattan(width: number): Heuristic {
	return (vertex, goal) => {
		const dx = math.abs((vertex % width) - (goal % width));
		const dy = math.abs(math.floor(vertex / width) - math.floor(goal / width));
		return dx + dy;
	};
}

export function chebyshev(width: number): Heuristic {
	return (vertex, goal) => {
		const dx = math.abs((vertex % width) - (goal % width));
		const dy = math.abs(math.floor(vertex / width) - math.floor(goal / width));
		return math.max(dx, dy);
	};
}

export interface SearchStats {
	readonly path: Array<number>;
	readonly expanded: number;
	readonly cost: number;
}

export function aStar(graph: Graph, start: number, goal: number, heuristic: Heuristic): SearchStats {
	const gScore = new Map<number, number>([[start, 0]]);
	const previous = new Map<number, number>();
	const closed = new Set<number>();
	const open = new MinQueue();
	open.push(start, heuristic(start, goal));
	let expanded = 0;
	while (!open.isEmpty()) {
		const { vertex } = open.pop()!;
		if (closed.has(vertex)) continue;
		closed.add(vertex);
		expanded++;
		if (vertex === goal) {
			return { path: reconstructPath(previous, start, goal), expanded, cost: gScore.get(goal)! };
		}
		const base = gScore.get(vertex)!;
		for (const edge of graph.neighbors(vertex)) {
			if (closed.has(edge.to)) continue;
			const tentative = base + edge.weight;
			if (tentative < (gScore.get(edge.to) ?? math.huge)) {
				gScore.set(edge.to, tentative);
				previous.set(edge.to, vertex);
				open.push(edge.to, tentative + heuristic(edge.to, goal));
			}
		}
	}
	return { path: [], expanded, cost: math.huge };
}

export function buildMaze(width: number, height: number, seed: number): Set<number> {
	const blocked = new Set<number>();
	let state = seed;
	for (let y = 0; y < height; y++) {
		for (let x = 0; x < width; x++) {
			state = (state * 1103515245 + 12345) % 2147483648;
			const isEdge = (x === 0 && y === 0) || (x === width - 1 && y === height - 1);
			if (!isEdge && state % 100 < 22) blocked.add(y * width + x);
		}
	}
	return blocked;
}

export function solveMaze(width: number, height: number, seed: number): SearchStats {
	const blocked = buildMaze(width, height, seed);
	const graph = Graph.grid(width, height, blocked);
	return aStar(graph, 0, width * height - 1, manhattan(width));
}

export function renderPath(width: number, height: number, blocked: ReadonlySet<number>, path: ReadonlyArray<number>): string {
	const onPath = new Set(path);
	const rows = new Array<string>();
	for (let y = 0; y < height; y++) {
		let row = "";
		for (let x = 0; x < width; x++) {
			const id = y * width + x;
			row += blocked.has(id) ? "#" : onPath.has(id) ? "*" : ".";
		}
		rows.push(row);
	}
	return rows.join("\n");
}

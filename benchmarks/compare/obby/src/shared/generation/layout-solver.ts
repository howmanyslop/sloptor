import { COURSE } from "../config";
import { CellState, gridPointKey, manhattan } from "../types";
import type { GridPoint } from "../types";
import { Grid3 } from "../util/grid";
import { PriorityQueue } from "../util/priority-queue";
import type { SeededRandom } from "../util/random";
import { err, ok } from "../util/result";
import type { Result } from "../util/result";
import { evaluateJump, isReachable, jumpCost } from "./reachability";

export interface SolverOptions {
	readonly cellSize: number;
	readonly maxIterations: number;
	readonly gapScale: number;
	readonly jitter: number;
}

export interface PathNode {
	readonly cell: GridPoint;
	readonly cost: number;
}

interface SearchState {
	readonly cell: GridPoint;
	readonly key: string;
	readonly g: number;
}

const DEFAULT_OPTIONS: SolverOptions = {
	cellSize: COURSE.gridCellSize,
	maxIterations: COURSE.maxSolverIterations,
	gapScale: 0.75,
	jitter: 0.35,
};

/** Candidate moves expressed in cells: forward-biased hops, sidesteps and drops. */
function buildMoveSet(gapScale: number): ReadonlyArray<GridPoint> {
	const maxForward = gapScale >= 0.8 ? 3 : 2;
	const moves = new Array<GridPoint>();
	for (let dz = 1; dz <= maxForward; dz++) {
		for (let dx = -1; dx <= 1; dx++) {
			for (let dy = -1; dy <= 1; dy++) {
				moves.push([dx, dy, dz]);
			}
		}
	}
	moves.push([1, 0, 0], [-1, 0, 0]);
	return moves;
}

export class LayoutSolver {
	private readonly options: SolverOptions;
	private readonly moves: ReadonlyArray<GridPoint>;
	private iterations = 0;

	public constructor(
		private readonly grid: Grid3<CellState>,
		private readonly random: SeededRandom,
		options: Partial<SolverOptions> = {},
	) {
		this.options = { ...DEFAULT_OPTIONS, ...options };
		this.moves = buildMoveSet(this.options.gapScale);
	}

	public getIterations(): number {
		return this.iterations;
	}

	public cellToWorld([x, y, z]: GridPoint): Vector3 {
		const size = this.options.cellSize;
		return new Vector3((x - this.grid.width / 2) * size, y * size * 0.5, z * size);
	}

	public solve(start: GridPoint, goal: GridPoint): Result<Array<PathNode>> {
		if (!this.grid.inBounds(start) || !this.grid.inBounds(goal)) {
			return err(`endpoints out of bounds: ${gridPointKey(start)} -> ${gridPointKey(goal)}`);
		}
		const open = new PriorityQueue<SearchState>();
		const cameFrom = new Map<string, string>();
		const cells = new Map<string, GridPoint>();
		const bestCost = new Map<string, number>();
		const startKey = gridPointKey(start);
		const goalKey = gridPointKey(goal);

		open.push({ cell: start, key: startKey, g: 0 }, manhattan(start, goal));
		bestCost.set(startKey, 0);
		cells.set(startKey, start);
		this.iterations = 0;

		while (!open.isEmpty()) {
			this.iterations += 1;
			if (this.iterations > this.options.maxIterations) {
				return err(`solver exceeded ${this.options.maxIterations} iterations`);
			}
			const current = open.pop()!;
			if (current.key === goalKey) {
				return ok(this.reconstruct(cameFrom, cells, bestCost, goalKey));
			}
			if (current.g > (bestCost.get(current.key) ?? math.huge)) {
				continue;
			}
			for (const neighbour of this.neighbours(current.cell)) {
				const stepCost = this.transitionCost(current.cell, neighbour);
				if (stepCost === math.huge) {
					continue;
				}
				const key = gridPointKey(neighbour);
				const tentative = current.g + stepCost + this.random.nextFloat() * this.options.jitter;
				if (tentative < (bestCost.get(key) ?? math.huge)) {
					bestCost.set(key, tentative);
					cameFrom.set(key, current.key);
					cells.set(key, neighbour);
					open.push({ cell: neighbour, key, g: tentative }, tentative + manhattan(neighbour, goal));
				}
			}
		}
		return err("no path between endpoints");
	}

	public commit(path: ReadonlyArray<PathNode>): void {
		for (const { cell } of path) {
			this.grid.set(cell, CellState.Path);
			const [x, y, z] = cell;
			for (let headroom = 1; headroom <= 2; headroom++) {
				const above: GridPoint = [x, y + headroom, z];
				if (this.grid.get(above) === CellState.Empty) {
					this.grid.set(above, CellState.Reserved);
				}
			}
		}
	}

	private *neighbours(cell: GridPoint): Generator<GridPoint> {
		const [x, y, z] = cell;
		for (const [dx, dy, dz] of this.moves) {
			const candidate: GridPoint = [x + dx, y + dy, z + dz];
			const state = this.grid.get(candidate);
			if (state === CellState.Empty || state === CellState.Path) {
				yield candidate;
			}
		}
	}

	private transitionCost(from: GridPoint, to: GridPoint): number {
		const verdict = evaluateJump(this.cellToWorld(from), this.cellToWorld(to), this.options.cellSize * 0.3);
		if (!isReachable(verdict)) {
			return math.huge;
		}
		// Prefer jumps near the tier's target gap so easy stages stay forgiving.
		const desired = this.options.gapScale;
		const actual = verdict.status === "jump" ? 1 - verdict.margin : 0;
		return jumpCost(verdict) + math.abs(desired - actual) * 2;
	}

	private reconstruct(
		cameFrom: ReadonlyMap<string, string>,
		cells: ReadonlyMap<string, GridPoint>,
		costs: ReadonlyMap<string, number>,
		goalKey: string,
	): Array<PathNode> {
		const reversed = new Array<PathNode>();
		let key: string | undefined = goalKey;
		while (key !== undefined) {
			const cell = cells.get(key);
			if (cell === undefined) {
				break;
			}
			reversed.push({ cell, cost: costs.get(key) ?? 0 });
			key = cameFrom.get(key);
		}
		const path = new Array<PathNode>();
		for (let index = reversed.size() - 1; index >= 0; index--) {
			path.push(reversed[index]);
		}
		return path;
	}
}

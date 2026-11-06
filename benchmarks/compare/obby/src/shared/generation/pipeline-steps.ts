import { COURSE } from "../config";
import { CellState, ObstacleKind, gridPointKey } from "../types";
import type { GridPoint, ObstacleSpec, SegmentSpec } from "../types";
import { chunk, enumerate, pairwise } from "../util/iter";
import { err, ok } from "../util/result";
import type { Result } from "../util/result";
import { buildKindPicker, generateParams, profileOf } from "./blueprints";
import { LayoutSolver } from "./layout-solver";
import type { PathNode } from "./layout-solver";
import { evaluateJump, isReachable } from "./reachability";
import { DiagnosticLevel, makeObstacleId, report } from "./stage-context";
import type { StageContext } from "./stage-context";

export abstract class PipelineStep {
	public abstract readonly name: string;

	/** Steps marked optional log failures as warnings instead of aborting. */
	public readonly optional: boolean = false;

	public abstract run(context: StageContext): Result<void>;

	public execute(context: StageContext): Result<void> {
		const started = os.clock();
		const result = this.run(context);
		const elapsedMs = (os.clock() - started) * 1000;
		if (result.success) {
			report(context, DiagnosticLevel.Info, this.name, `ok in ${string.format("%.2f", elapsedMs)}ms`);
			return result;
		}
		const level = this.optional ? DiagnosticLevel.Warning : DiagnosticLevel.Failure;
		report(context, level, this.name, result.error);
		return this.optional ? ok(undefined) : result;
	}
}

export class ReserveTerrainStep extends PipelineStep {
	public readonly name = "reserve-terrain";

	public run(context: StageContext): Result<void> {
		const { grid, random, difficulty } = context;
		const pillarCount = math.floor(2 + difficulty * 10);
		let placed = 0;
		for (let attempt = 0; attempt < pillarCount * 4 && placed < pillarCount; attempt++) {
			const origin: GridPoint = [random.nextInt(0, grid.width - 2), 0, random.nextInt(4, grid.depth - 6)];
			const size: GridPoint = [random.nextInt(1, 2), random.nextInt(2, grid.height - 2), random.nextInt(1, 2)];
			if (grid.isBoxClear(origin, size, (state) => state === CellState.Empty)) {
				grid.fillBox(origin, size, CellState.Solid);
				placed += 1;
			}
		}
		return ok(undefined);
	}
}

export class SolveLayoutStep extends PipelineStep {
	public readonly name = "solve-layout";

	public run(context: StageContext): Result<void> {
		const solver = new LayoutSolver(context.grid, context.random.fork("layout"), {
			gapScale: context.settings.gapScale,
			jitter: 0.2 + context.difficulty * 0.4,
		});
		const result = solver.solve(context.entry, context.exit);
		if (!result.success) {
			return err(`${result.error} after ${solver.getIterations()} iterations`);
		}
		solver.commit(result.value);
		context.path = result.value;
		context.estimatedSeconds = result.value.reduce((total, node, index) => total + (index === 0 ? 0 : 1.1), 0);
		return ok(undefined);
	}
}

export class PopulateObstaclesStep extends PipelineStep {
	public readonly name = "populate-obstacles";

	public run(context: StageContext): Result<void> {
		const { random, difficulty, settings, path, stageIndex } = context;
		if (path.size() < 2) {
			return err("path too short to populate");
		}
		const picker = buildKindPicker(difficulty, settings.hazardRatio);
		const budget = math.min(settings.obstacleBudget, path.size() - 1);
		const occupied = new Set<string>();
		const obstacles = new Array<ObstacleSpec>();
		const usableNodes = random.shuffle(path.filter((_, index) => index > 0 && index < path.size() - 1));

		for (const [ordinal, node] of enumerate(usableNodes)) {
			if (obstacles.size() >= budget) {
				break;
			}
			const key = gridPointKey(node.cell);
			if (occupied.has(key)) {
				continue;
			}
			const kind = picker.pick(random);
			if (kind === undefined) {
				return err("kind picker is empty");
			}
			const profile = profileOf(kind);
			occupied.add(key);
			obstacles.push({
				id: makeObstacleId(stageIndex, ordinal),
				kind,
				cell: node.cell,
				footprint: profile.footprint,
				rotation: random.pick([0, 90, 180, 270]) ?? 0,
				params: generateParams(kind, { random, difficulty, tier: context.tier, stageIndex }),
			});
		}

		// Every unoccupied path node still needs something to stand on.
		path.forEach((node, index) => {
			if (occupied.has(gridPointKey(node.cell))) {
				return;
			}
			obstacles.push({
				id: makeObstacleId(stageIndex, 1000 + index, "floor"),
				kind: ObstacleKind.Platform,
				cell: node.cell,
				footprint: [1, 1, 1],
				rotation: 0,
				params: { kind: ObstacleKind.Platform, size: new Vector3(7, 1, 7), material: Enum.Material.SmoothPlastic },
			});
		});

		context.obstacles = obstacles;
		return ok(undefined);
	}
}

export class SafeLandingStep extends PipelineStep {
	public readonly name = "safe-landing";
	public readonly optional = true;

	public run(context: StageContext): Result<void> {
		const pathKeys = new Set(context.path.map((node) => gridPointKey(node.cell)));
		let replaced = 0;
		const adjusted = new Array<ObstacleSpec>();
		for (const spec of context.obstacles) {
			const landsOnHazard = spec.params.kind === ObstacleKind.KillBrick && pathKeys.has(gridPointKey(spec.cell));
			if (!landsOnHazard) {
				adjusted.push(spec);
				continue;
			}
			replaced += 1;
			// Keep the hazard beside the landing cell and give the player a floor.
			const [x, y, z] = spec.cell;
			const sideX = context.grid.get([x + 1, y, z]) === CellState.Empty ? x + 1 : x - 1;
			adjusted.push({ ...spec, cell: [sideX, y, z] });
			adjusted.push({
				...spec,
				id: `${spec.id}_floor`,
				kind: ObstacleKind.Platform,
				params: { kind: ObstacleKind.Platform, size: new Vector3(6, 1, 6), material: Enum.Material.Slate },
			});
		}
		context.obstacles = adjusted;
		return replaced > 0 ? err(`moved ${replaced} kill bricks off the critical path`) : ok(undefined);
	}
}

export class SegmentStep extends PipelineStep {
	public readonly name = "segment";

	public run(context: StageContext): Result<void> {
		const perSegment = math.max(2, math.ceil(context.path.size() / COURSE.segmentsPerStage));
		const byCell = new Map<string, Array<ObstacleSpec>>();
		for (const spec of context.obstacles) {
			const key = gridPointKey(spec.cell);
			const bucket = byCell.get(key) ?? [];
			bucket.push(spec);
			byCell.set(key, bucket);
		}
		const segments = new Array<SegmentSpec>();
		for (const [index, nodes] of enumerate(collectChunks(context, perSegment))) {
			const first = nodes[0];
			const last = nodes[nodes.size() - 1];
			const obstacles = new Array<ObstacleSpec>();
			for (const node of nodes) {
				for (const spec of byCell.get(gridPointKey(node.cell)) ?? []) {
					obstacles.push(spec);
				}
			}
			segments.push({ index, tier: context.tier, entry: first.cell, exit: last.cell, obstacles });
		}
		context.segments = segments;
		return segments.isEmpty() ? err("no segments produced") : ok(undefined);
	}
}

function collectChunks(context: StageContext, size: number): Array<Array<PathNode>> {
	const result = new Array<Array<PathNode>>();
	for (const group of chunk(context.path, size)) {
		result.push(group);
	}
	return result;
}

export class CheckpointStep extends PipelineStep {
	public readonly name = "checkpoint";

	public run(context: StageContext): Result<void> {
		const last = context.path[context.path.size() - 1];
		if (last === undefined) {
			return err("cannot place checkpoint without a path");
		}
		context.checkpoint = {
			id: makeObstacleId(context.stageIndex, 0, "checkpoint"),
			kind: ObstacleKind.Checkpoint,
			cell: last.cell,
			footprint: [1, 1, 1],
			rotation: 0,
			params: { kind: ObstacleKind.Checkpoint, stageIndex: context.stageIndex },
		};
		context.obstacles = context.obstacles.filter((spec) => gridPointKey(spec.cell) !== gridPointKey(last.cell));
		return ok(undefined);
	}
}

export class ValidateReachabilityStep extends PipelineStep {
	public readonly name = "validate-reachability";

	public constructor(private readonly toWorld: (cell: GridPoint) => Vector3) {
		super();
	}

	public run(context: StageContext): Result<void> {
		const failures = new Array<string>();
		for (const [previous, current] of pairwise(context.path)) {
			const verdict = evaluateJump(this.toWorld(previous.cell), this.toWorld(current.cell), COURSE.gridCellSize * 0.3);
			if (!isReachable(verdict)) {
				failures.push(`${gridPointKey(previous.cell)} -> ${gridPointKey(current.cell)} (${verdict.status})`);
			}
		}
		if (!failures.isEmpty()) {
			return err(`unreachable hops: ${failures.join("; ")}`);
		}
		const hazardsOnPath = context.obstacles.filter(
			(spec) => spec.kind === ObstacleKind.KillBrick && context.path.some((node) => gridPointKey(node.cell) === gridPointKey(spec.cell)),
		);
		return hazardsOnPath.size() > context.settings.obstacleBudget
			? err(`${hazardsOnPath.size()} hazards exceed budget`)
			: ok(undefined);
	}
}

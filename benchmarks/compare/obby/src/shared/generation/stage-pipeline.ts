import { COURSE, TIER_SETTINGS } from "../config";
import { CellState } from "../types";
import type { DifficultyTier, GridPoint, StageSpec } from "../types";
import { Grid3 } from "../util/grid";
import type { SeededRandom } from "../util/random";
import { err, ok } from "../util/result";
import type { Result } from "../util/result";
import {
	CheckpointStep,
	PipelineStep,
	PopulateObstaclesStep,
	ReserveTerrainStep,
	SafeLandingStep,
	SegmentStep,
	SolveLayoutStep,
	ValidateReachabilityStep,
} from "./pipeline-steps";
import { DiagnosticLevel, hasFailures } from "./stage-context";
import type { Diagnostic, StageContext } from "./stage-context";

export interface StageRequest {
	readonly stageIndex: number;
	readonly difficulty: number;
	readonly tier: DifficultyTier;
	readonly entryX: number;
	readonly worldOffset: Vector3;
}

export interface StageOutcome {
	readonly stage: StageSpec;
	readonly attempts: number;
	readonly diagnostics: ReadonlyArray<Diagnostic>;
	readonly exitX: number;
}

export function cellToStageLocal([x, y, z]: GridPoint): Vector3 {
	const size = COURSE.gridCellSize;
	return new Vector3((x - COURSE.gridWidth / 2) * size, y * size * 0.5, z * size);
}

export class StagePipeline {
	private readonly steps: ReadonlyArray<PipelineStep>;
	private totalAttempts = 0;

	public constructor(steps?: ReadonlyArray<PipelineStep>) {
		this.steps = steps ?? StagePipeline.defaultSteps();
	}

	public static defaultSteps(): Array<PipelineStep> {
		return [
			new ReserveTerrainStep(),
			new SolveLayoutStep(),
			new PopulateObstaclesStep(),
			new SafeLandingStep(),
			new CheckpointStep(),
			new SegmentStep(),
			new ValidateReachabilityStep(cellToStageLocal),
		];
	}

	public getStepNames(): Array<string> {
		return this.steps.map((step) => step.name);
	}

	public getTotalAttempts(): number {
		return this.totalAttempts;
	}

	public build(request: StageRequest, random: SeededRandom): Result<StageOutcome> {
		const history = new Array<Diagnostic>();
		for (let attempt = 1; attempt <= COURSE.maxRegenerationAttempts; attempt++) {
			this.totalAttempts += 1;
			const context = this.createContext(request, random.fork(`stage${request.stageIndex}-attempt${attempt}`));
			const result = this.runSteps(context);
			for (const diagnostic of context.diagnostics) {
				history.push(diagnostic);
			}
			if (result.success && context.checkpoint !== undefined) {
				const stage: StageSpec = {
					index: request.stageIndex,
					tier: request.tier,
					difficulty: request.difficulty,
					segments: context.segments,
					checkpoint: context.checkpoint,
					worldOffset: request.worldOffset,
					estimatedSeconds: context.estimatedSeconds,
				};
				return ok({ stage, attempts: attempt, diagnostics: history, exitX: context.exit[0] });
			}
		}
		const failures = history
			.filter((diagnostic) => diagnostic.level === DiagnosticLevel.Failure)
			.map(({ step, message }) => `[${step}] ${message}`);
		return err(`stage ${request.stageIndex} failed after ${COURSE.maxRegenerationAttempts} attempts: ${failures.join(" | ")}`);
	}

	private createContext(request: StageRequest, random: SeededRandom): StageContext {
		const grid = new Grid3<CellState>(COURSE.gridWidth, COURSE.gridHeight, COURSE.gridDepth, CellState.Empty);
		const exitX = math.clamp(request.entryX + random.nextInt(-3, 3), 1, COURSE.gridWidth - 2);
		const exitY = math.clamp(math.floor(request.difficulty * (COURSE.gridHeight - 2)), 0, COURSE.gridHeight - 3);
		return {
			stageIndex: request.stageIndex,
			tier: request.tier,
			difficulty: request.difficulty,
			settings: TIER_SETTINGS[request.tier],
			random,
			grid,
			worldOffset: request.worldOffset,
			entry: [request.entryX, 0, 0],
			exit: [exitX, exitY, COURSE.gridDepth - 1],
			path: [],
			obstacles: [],
			segments: [],
			estimatedSeconds: 0,
			diagnostics: [],
		};
	}

	private runSteps(context: StageContext): Result<void> {
		for (const step of this.steps) {
			const result = step.execute(context);
			if (!result.success) {
				return result;
			}
		}
		return hasFailures(context) ? err("pipeline reported failures") : ok(undefined);
	}
}

import { COURSE } from "../config";
import type { CourseSpec, StageSpec } from "../types";
import { SeededRandom } from "../util/random";
import { err, ok } from "../util/result";
import type { Result } from "../util/result";
import { Signal } from "../util/signal";
import { DifficultyCurve } from "./difficulty-curve";
import type { CurveShape } from "./difficulty-curve";
import { DiagnosticLevel } from "./stage-context";
import { StagePipeline } from "./stage-pipeline";

export interface GenerateOptions {
	readonly seed: number;
	readonly label?: string;
	readonly stageCount?: number;
	readonly curve?: CurveShape;
	readonly ceiling?: number;
	readonly timestamp?: number;
}

export interface GenerationReport {
	readonly course: CourseSpec;
	readonly totalAttempts: number;
	readonly warnings: ReadonlyArray<string>;
	readonly elapsedSeconds: number;
}

const STAGE_SPACING = 24;

export class CourseGenerator {
	public readonly stageGenerated = new Signal<[stage: StageSpec, attempts: number]>();
	public readonly generationFailed = new Signal<[reason: string]>();
	private readonly pipeline: StagePipeline;

	public constructor(pipeline: StagePipeline = new StagePipeline()) {
		this.pipeline = pipeline;
	}

	public generate(options: GenerateOptions): Result<GenerationReport> {
		const started = os.clock();
		const {
			seed,
			label = `course-${seed}`,
			stageCount = COURSE.stageCount,
			curve = DifficultyCurve.DEFAULT_SHAPE,
			ceiling = 1,
			timestamp = os.time(),
		} = options;

		const random = new SeededRandom(seed);
		const samples = new DifficultyCurve(curve, 0.05, ceiling).sample(stageCount);
		const stages = new Array<StageSpec>();
		const warnings = new Array<string>();
		let entryX = math.floor(COURSE.gridWidth / 2);
		let offset = Vector3.zero;
		const attemptsBefore = this.pipeline.getTotalAttempts();

		for (const sample of samples) {
			const result = this.pipeline.build(
				{
					stageIndex: sample.stageIndex,
					difficulty: sample.difficulty,
					tier: sample.tier,
					entryX,
					worldOffset: offset,
				},
				random,
			);
			if (!result.success) {
				this.generationFailed.fire(result.error);
				return err(result.error);
			}
			const { stage, attempts, diagnostics, exitX } = result.value;
			for (const diagnostic of diagnostics) {
				if (diagnostic.level === DiagnosticLevel.Warning) {
					warnings.push(`stage ${stage.index} [${diagnostic.step}] ${diagnostic.message}`);
				}
			}
			stages.push(stage);
			this.stageGenerated.fire(stage, attempts);
			entryX = exitX;
			offset = offset.add(new Vector3(0, 0, COURSE.gridDepth * COURSE.gridCellSize + STAGE_SPACING));
		}

		const totalAttempts = this.pipeline.getTotalAttempts() - attemptsBefore;
		return ok({
			course: { seed, label, stages, generatedAt: timestamp },
			totalAttempts,
			warnings,
			elapsedSeconds: os.clock() - started,
		});
	}

	public destroy(): void {
		this.stageGenerated.destroy();
		this.generationFailed.destroy();
	}
}

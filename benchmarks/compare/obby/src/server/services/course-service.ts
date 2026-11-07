import { CourseGenerator } from "shared/generation/course-generator";
import type { GenerationReport } from "shared/generation/course-generator";
import { describeCourse } from "shared/generation/course-stats";
import { dailyCourseFor } from "shared/generation/daily-seed";
import type { DailyCourseInfo } from "shared/generation/daily-seed";
import type { CourseSpec, StageSpec } from "shared/types";
import { Signal } from "shared/util/signal";
import { Service } from "./service";

export type CourseMode = { readonly type: "daily"; readonly info: DailyCourseInfo } | { readonly type: "custom"; readonly seed: number };

export class CourseService extends Service {
	public readonly courseReady = new Signal<[course: CourseSpec, mode: CourseMode]>();
	public readonly stageReady = new Signal<[stage: StageSpec]>();

	private readonly generator = new CourseGenerator();
	private current?: CourseSpec;
	private currentMode?: CourseMode;
	private lastReport?: GenerationReport;
	private generating = false;

	public constructor() {
		super("CourseService");
	}

	protected getPriority(): number {
		return 100;
	}

	public init(): void {
		this.generator.stageGenerated.connect((stage, attempts) => {
			if (attempts > 1) {
				print(`[course] stage ${stage.index + 1} needed ${attempts} attempts`);
			}
			this.stageReady.fire(stage);
		});
		this.generator.generationFailed.connect((reason) => warn(`[course] generation failed: ${reason}`));
	}

	public start(): void {
		this.loadDaily(os.time());
	}

	public getCourse(): CourseSpec | undefined {
		return this.current;
	}

	public getMode(): CourseMode | undefined {
		return this.currentMode;
	}

	public getLastReport(): GenerationReport | undefined {
		return this.lastReport;
	}

	public getStage(index: number): StageSpec | undefined {
		return this.current?.stages[index];
	}

	public getStageCount(): number {
		return this.current?.stages.size() ?? 0;
	}

	public loadDaily(timestamp: number): boolean {
		const info = dailyCourseFor(timestamp);
		return this.load({ type: "daily", info });
	}

	public loadCustom(seed: number): boolean {
		return this.load({ type: "custom", seed });
	}

	private load(mode: CourseMode): boolean {
		if (this.generating) {
			warn("[course] generation already in progress");
			return false;
		}
		this.generating = true;
		const options =
			mode.type === "daily"
				? {
						seed: mode.info.seed,
						label: `Daily ${mode.info.key} - ${mode.info.theme.name}`,
						stageCount: mode.info.stageCount,
						curve: mode.info.curve,
					}
				: { seed: mode.seed, label: `Custom #${mode.seed}` };
		const result = this.generator.generate(options);
		this.generating = false;
		if (!result.success) {
			return false;
		}
		const report = result.value;
		this.lastReport = report;
		this.current = report.course;
		this.currentMode = mode;
		for (const line of describeCourse(report.course)) {
			print(line);
		}
		for (const warning of report.warnings) {
			warn(`[course] ${warning}`);
		}
		print(
			`[course] generated in ${string.format("%.1f", report.elapsedSeconds * 1000)}ms with ${report.totalAttempts} pipeline runs`,
		);
		this.courseReady.fire(report.course, mode);
		return true;
	}

	public shutdown(): void {
		this.generator.destroy();
		this.courseReady.destroy();
		this.stageReady.destroy();
	}
}

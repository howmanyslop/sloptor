import type { CourseSpec, DifficultyTier, ObstacleKind, ObstacleSpec, StageSpec } from "../types";
import { countBy, flatMap, maxBy } from "../util/iter";
import { mean, percentile, sum } from "../util/math";

export interface StageSummary {
	readonly index: number;
	readonly tier: DifficultyTier;
	readonly obstacleCount: number;
	readonly hazardCount: number;
	readonly estimatedSeconds: number;
}

export interface CourseSummary {
	readonly stageCount: number;
	readonly obstacleCount: number;
	readonly kindCounts: ReadonlyMap<ObstacleKind, number>;
	readonly tierCounts: ReadonlyMap<DifficultyTier, number>;
	readonly estimatedSeconds: number;
	readonly medianStageSeconds: number;
	readonly hardestStage?: StageSummary;
	readonly allStagesHaveCheckpoints: boolean;
}

const HAZARD_KINDS = new Set<string>(["KillBrick", "Laser", "Spinner"]);

export function stageObstacles(stage: StageSpec): Array<ObstacleSpec> {
	return [...flatMap(stage.segments, (segment) => segment.obstacles), stage.checkpoint];
}

export function summarizeStage(stage: StageSpec): StageSummary {
	const obstacles = stageObstacles(stage);
	return {
		index: stage.index,
		tier: stage.tier,
		obstacleCount: obstacles.size(),
		hazardCount: obstacles.filter((spec) => HAZARD_KINDS.has(spec.kind)).size(),
		estimatedSeconds: stage.estimatedSeconds,
	};
}

export function summarizeCourse(course: CourseSpec): CourseSummary {
	const stages = course.stages.map(summarizeStage);
	const obstacles = flatMap(course.stages, stageObstacles);
	const stageSeconds = stages.map((stage) => stage.estimatedSeconds);
	return {
		stageCount: stages.size(),
		obstacleCount: obstacles.size(),
		kindCounts: countBy(obstacles, (spec) => spec.kind),
		tierCounts: countBy(course.stages, (stage) => stage.tier),
		estimatedSeconds: sum(stageSeconds),
		medianStageSeconds: percentile(stageSeconds, 0.5),
		hardestStage: maxBy(stages, (stage) => stage.hazardCount * 2 + stage.obstacleCount),
		allStagesHaveCheckpoints: course.stages.every((stage) => stage.checkpoint !== undefined),
	};
}

export function describeCourse(course: CourseSpec): Array<string> {
	const summary = summarizeCourse(course);
	const lines = [
		`Course "${course.label}" (seed ${course.seed})`,
		`  stages: ${summary.stageCount}, obstacles: ${summary.obstacleCount}`,
		`  estimated: ${string.format("%.1f", summary.estimatedSeconds)}s (median stage ${string.format("%.1f", summary.medianStageSeconds)}s)`,
	];
	for (const [kind, count] of summary.kindCounts) {
		lines.push(`  ${kind}: ${count}`);
	}
	const hardest = summary.hardestStage;
	if (hardest) {
		lines.push(`  hardest: stage ${hardest.index + 1} (${hardest.tier}, ${hardest.hazardCount} hazards)`);
	}
	const averageHazards = mean(course.stages.map((stage) => summarizeStage(stage).hazardCount));
	lines.push(`  avg hazards/stage: ${string.format("%.2f", averageHazards)}`);
	return lines;
}

import { Workspace } from "@rbxts/services";
import { COURSE, COURSE_FOLDER_NAME, TIER_SETTINGS } from "shared/config";
import { stageObstacles } from "shared/generation/course-stats";
import { cellToStageLocal } from "shared/generation/stage-pipeline";
import type { Checkpoint } from "shared/obstacles/checkpoint";
import { Obstacle } from "shared/obstacles/obstacle";
import { ObstacleRegistry, createObstacle, isCheckpoint } from "shared/obstacles/registry";
import type { CourseSpec, StageSpec } from "shared/types";
import { Signal } from "shared/util/signal";
import type { CourseService } from "./course-service";
import { Service } from "./service";

export interface BuiltStage {
	readonly spec: StageSpec;
	readonly folder: Folder;
	readonly checkpoint?: Checkpoint;
	readonly obstacleIds: ReadonlyArray<string>;
	readonly origin: CFrame;
}

const COURSE_ORIGIN = new CFrame(0, 40, 0);
const BUILD_BUDGET_PER_FRAME = 40;

export class StageBuilder extends Service {
	public readonly stageBuilt = new Signal<[stage: BuiltStage]>();
	public readonly courseBuilt = new Signal<[stages: ReadonlyArray<BuiltStage>]>();

	private readonly registry = new ObstacleRegistry();
	private readonly built = new Map<number, BuiltStage>();
	private root?: Folder;
	private spawnPad?: SpawnLocation;
	private buildToken = 0;

	public constructor(private readonly courses: CourseService) {
		super("StageBuilder");
	}

	protected getPriority(): number {
		return 90;
	}

	public init(): void {
		this.root = this.ensureRoot();
	}

	public start(): void {
		this.courses.courseReady.connect((course) => this.rebuild(course));
		const existing = this.courses.getCourse();
		if (existing) {
			this.rebuild(existing);
		}
	}

	public getRegistry(): ObstacleRegistry {
		return this.registry;
	}

	public getBuiltStage(index: number): BuiltStage | undefined {
		return this.built.get(index);
	}

	public getSpawnCFrame(stageIndex: number): CFrame {
		if (stageIndex <= 0) {
			return (this.spawnPad?.CFrame ?? COURSE_ORIGIN).add(new Vector3(0, 4, 0));
		}
		return this.built.get(stageIndex - 1)?.checkpoint?.getSpawnCFrame() ?? COURSE_ORIGIN.add(new Vector3(0, 6, 0));
	}

	public rebuild(course: CourseSpec): void {
		this.buildToken += 1;
		const token = this.buildToken;
		this.clear();
		const root = this.ensureRoot();
		root.SetAttribute("Seed", course.seed);
		root.SetAttribute("Label", course.label);
		this.spawnPad = this.createSpawn(root);

		const results = new Array<BuiltStage>();
		let budget = BUILD_BUDGET_PER_FRAME;
		for (const stage of course.stages) {
			if (token !== this.buildToken) {
				return;
			}
			const builtStage = this.buildStage(stage, root);
			results.push(builtStage);
			this.built.set(stage.index, builtStage);
			this.stageBuilt.fire(builtStage);
			budget -= builtStage.obstacleIds.size();
			if (budget <= 0) {
				budget = BUILD_BUDGET_PER_FRAME;
				task.wait();
			}
		}
		print(`[builder] built ${results.size()} stages, ${this.registry.size()} obstacles`);
		this.courseBuilt.fire(results);
	}

	public clear(): void {
		this.registry.clear();
		this.built.clear();
		this.root?.ClearAllChildren();
		this.spawnPad = undefined;
		Obstacle.resetSerials();
	}

	public shutdown(): void {
		this.clear();
		this.stageBuilt.destroy();
		this.courseBuilt.destroy();
	}

	private buildStage(stage: StageSpec, root: Folder): BuiltStage {
		const folder = new Instance("Folder");
		folder.Name = string.format("Stage%02d", stage.index + 1);
		folder.SetAttribute("Tier", stage.tier);
		folder.SetAttribute("Difficulty", stage.difficulty);
		folder.Parent = root;

		const origin = COURSE_ORIGIN.add(stage.worldOffset);
		const accent = TIER_SETTINGS[stage.tier].color;
		const obstacleIds = new Array<string>();
		let checkpoint: Checkpoint | undefined;

		for (const spec of stageObstacles(stage)) {
			const obstacle = createObstacle(spec);
			const localOffset = cellToStageLocal(spec.cell);
			obstacle.build({
				parent: folder,
				origin: origin.add(localOffset),
				accent,
				cellSize: COURSE.gridCellSize,
			});
			this.registry.register(obstacle);
			obstacleIds.push(spec.id);
			if (isCheckpoint(obstacle)) {
				checkpoint = obstacle;
			}
		}
		this.buildBoundary(stage, folder, origin);
		return { spec: stage, folder, checkpoint, obstacleIds, origin };
	}

	private buildBoundary(stage: StageSpec, folder: Folder, origin: CFrame): void {
		const length = COURSE.gridDepth * COURSE.gridCellSize;
		const width = COURSE.gridWidth * COURSE.gridCellSize;
		const killFloor = new Instance("Part");
		killFloor.Name = "KillFloor";
		killFloor.Anchored = true;
		killFloor.CanCollide = false;
		killFloor.Transparency = 1;
		killFloor.Size = new Vector3(width + 40, 2, length + 40);
		killFloor.CFrame = origin.add(new Vector3(0, -30, length / 2));
		killFloor.SetAttribute("HazardResponse", "kill");
		killFloor.SetAttribute("StageIndex", stage.index);
		killFloor.Parent = folder;
	}

	private createSpawn(root: Folder): SpawnLocation {
		const spawn = new Instance("SpawnLocation");
		spawn.Name = "CourseStart";
		spawn.Anchored = true;
		spawn.Size = new Vector3(12, 1, 12);
		spawn.CFrame = COURSE_ORIGIN.add(new Vector3(0, 0, -12));
		spawn.Duration = 0;
		spawn.Neutral = true;
		spawn.Material = Enum.Material.Neon;
		spawn.Color = Color3.fromRGB(255, 255, 255);
		spawn.Parent = root;
		return spawn;
	}

	private ensureRoot(): Folder {
		if (this.root && this.root.Parent !== undefined) {
			return this.root;
		}
		let folder = Workspace.FindFirstChild(COURSE_FOLDER_NAME) as Folder | undefined;
		if (folder === undefined) {
			folder = new Instance("Folder");
			folder.Name = COURSE_FOLDER_NAME;
			folder.Parent = Workspace;
		}
		this.root = folder;
		return folder;
	}
}

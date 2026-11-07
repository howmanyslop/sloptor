import { Players } from "@rbxts/services";
import { RESPAWN_DELAY, TIER_SETTINGS } from "shared/config";
import { RemoteName, fireClient } from "shared/network/remotes";
import { CHECKPOINT_ATTRIBUTE } from "shared/obstacles/checkpoint";
import { recordStageTime } from "shared/progress/progress-data";
import { Janitor } from "shared/util/signal";
import type { CourseService } from "./course-service";
import type { ProgressStore } from "./progress-store";
import { Service } from "./service";
import type { PlayerSession, SessionService } from "./session-service";
import type { BuiltStage, StageBuilder } from "./stage-builder";

const TOUCH_DEBOUNCE = 0.75;

export class CheckpointService extends Service {
	private readonly janitor = new Janitor();
	private readonly lastTouch = new Map<Player, number>();

	public constructor(
		private readonly courses: CourseService,
		private readonly builder: StageBuilder,
		private readonly sessions: SessionService,
		private readonly progress: ProgressStore,
	) {
		super("CheckpointService");
	}

	public init(): void {}

	public start(): void {
		this.janitor.add(this.builder.stageBuilt.connect((stage) => this.bindStage(stage)));
		this.janitor.add(
			this.sessions.splitRecorded.connect((session, split) => {
				const updated = this.progress.update(session.player, (data) =>
					recordStageTime(data, split.stageIndex, split.seconds, split.deaths),
				);
				const best = updated?.stageRecords[tostring(split.stageIndex)]?.bestSeconds;
				fireClient(session.player, RemoteName.StageReached, {
					stageIndex: split.stageIndex,
					splitSeconds: split.seconds,
					personalBest: best,
				});
				if (split.stageIndex + 1 >= this.courses.getStageCount()) {
					this.sessions.finishRun(session);
				}
			}),
		);
		this.janitor.add(
			this.sessions.sessionStarted.connect((session) => {
				session.player.CharacterAdded.Connect((character) => this.placeAtCheckpoint(session, character));
			}),
		);
		this.janitor.add(Players.PlayerRemoving.Connect((player) => this.lastTouch.delete(player)));
	}

	public respawn(player: Player): void {
		const session = this.sessions.get(player);
		if (!session) {
			return;
		}
		task.delay(RESPAWN_DELAY, () => {
			if (player.Parent !== undefined) {
				player.LoadCharacter();
			}
		});
	}

	public shutdown(): void {
		this.janitor.cleanup();
	}

	private bindStage(stage: BuiltStage): void {
		const pad = stage.checkpoint?.getModel()?.FindFirstChild("Pad") as BasePart | undefined;
		if (!pad) {
			return;
		}
		const stageIndex = pad.GetAttribute(CHECKPOINT_ATTRIBUTE) as number | undefined;
		if (stageIndex === undefined) {
			return;
		}
		pad.Touched.Connect((hit) => {
			const character = hit.Parent;
			const player = character ? Players.GetPlayerFromCharacter(character) : undefined;
			if (player === undefined) {
				return;
			}
			const now = os.clock();
			if (now - (this.lastTouch.get(player) ?? 0) < TOUCH_DEBOUNCE) {
				return;
			}
			this.lastTouch.set(player, now);
			const session = this.sessions.get(player);
			if (session && session.tracker.reachCheckpoint(stageIndex)) {
				stage.checkpoint?.markReached(TIER_SETTINGS[stage.spec.tier].color);
			}
		});
	}

	private placeAtCheckpoint(session: PlayerSession, character: Model): void {
		const root = character.WaitForChild("HumanoidRootPart", 5) as BasePart | undefined;
		if (!root) {
			return;
		}
		const target = this.builder.getSpawnCFrame(session.respawnStage);
		task.defer(() => character.PivotTo(target));
	}
}

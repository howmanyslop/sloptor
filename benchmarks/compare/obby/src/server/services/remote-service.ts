import { Players } from "@rbxts/services";
import { RemoteName, fireClient, isEmptyPayload, isRestartRequest, isSettingsUpdate, onServerEvent } from "shared/network/remotes";
import type { PlayerSettings } from "shared/progress/progress-data";
import { Janitor } from "shared/util/signal";
import type { CheckpointService } from "./checkpoint-service";
import type { CourseService } from "./course-service";
import type { ProgressStore } from "./progress-store";
import { Service } from "./service";
import type { SessionService } from "./session-service";

const RATE_LIMIT_WINDOW = 1;
const RATE_LIMIT_MAX = 6;

const SETTING_KEYS: ReadonlySet<keyof PlayerSettings> = new Set(["showTimer", "showGhostPath", "reducedMotion"]);

function isSettingKey(key: string): key is keyof PlayerSettings {
	return SETTING_KEYS.has(key as keyof PlayerSettings);
}

export class RemoteService extends Service {
	private readonly janitor = new Janitor();
	private readonly requestLog = new Map<Player, Array<number>>();

	public constructor(
		private readonly courses: CourseService,
		private readonly sessions: SessionService,
		private readonly checkpoints: CheckpointService,
		private readonly progress: ProgressStore,
	) {
		super("RemoteService");
	}

	public init(): void {}

	public start(): void {
		this.janitor.add(
			onServerEvent(RemoteName.RequestRespawn, isEmptyPayload, (player) => {
				if (this.allow(player)) {
					this.checkpoints.respawn(player);
				}
			}),
		);
		this.janitor.add(
			onServerEvent(RemoteName.RequestRestart, isRestartRequest, (player, { daily }) => {
				if (!this.allow(player)) {
					return;
				}
				const session = this.sessions.restart(player, daily);
				if (session) {
					player.LoadCharacter();
				}
			}),
		);
		this.janitor.add(
			onServerEvent(RemoteName.UpdateSettings, isSettingsUpdate, (player, { key, value }) => {
				if (!this.allow(player) || !isSettingKey(key)) {
					return;
				}
				this.progress.update(player, (data) => ({ ...data, settings: { ...data.settings, [key]: value } }));
			}),
		);
		this.janitor.add(this.courses.courseReady.connect(() => this.announceAll()));
		this.janitor.add(this.sessions.sessionStarted.connect((session) => this.announce(session.player)));
		this.janitor.add(Players.PlayerRemoving.Connect((player) => this.requestLog.delete(player)));
	}

	public shutdown(): void {
		this.janitor.cleanup();
	}

	private allow(player: Player): boolean {
		const now = os.clock();
		const recent = (this.requestLog.get(player) ?? []).filter((at) => now - at < RATE_LIMIT_WINDOW);
		if (recent.size() >= RATE_LIMIT_MAX) {
			this.requestLog.set(player, recent);
			return false;
		}
		recent.push(now);
		this.requestLog.set(player, recent);
		return true;
	}

	private announceAll(): void {
		for (const player of Players.GetPlayers()) {
			this.announce(player);
		}
	}

	private announce(player: Player): void {
		const course = this.courses.getCourse();
		const mode = this.courses.getMode();
		if (!course || !mode) {
			return;
		}
		fireClient(player, RemoteName.CourseAnnounced, {
			seed: course.seed,
			label: course.label,
			stageCount: course.stages.size(),
			themeName: mode.type === "daily" ? mode.info.theme.name : "Custom",
			tiers: course.stages.map((stage) => stage.tier),
			secondsUntilReset: mode.type === "daily" ? mode.info.secondsUntilReset : 0,
		});
	}
}

import { Players } from "@rbxts/services";
import { RunStatus, RunTracker } from "shared/progress/run-tracker";
import type { StageSplit } from "shared/progress/run-tracker";
import { Signal } from "shared/util/signal";
import { Service } from "./service";

export interface PlayerSession {
	readonly player: Player;
	readonly tracker: RunTracker;
	readonly joinedAt: number;
	respawnStage: number;
	jumps: number;
	isDaily: boolean;
}

export class SessionService extends Service {
	public readonly sessionStarted = new Signal<[session: PlayerSession]>();
	public readonly sessionEnded = new Signal<[session: PlayerSession]>();
	public readonly splitRecorded = new Signal<[session: PlayerSession, split: StageSplit]>();
	public readonly runFinished = new Signal<[session: PlayerSession]>();

	private readonly sessions = new Map<Player, PlayerSession>();

	public constructor() {
		super("SessionService");
	}

	protected getPriority(): number {
		return 70;
	}

	public init(): void {}

	public start(): void {
		Players.PlayerAdded.Connect((player) => this.open(player));
		Players.PlayerRemoving.Connect((player) => this.close(player));
		Players.GetPlayers().forEach((player) => this.open(player));
	}

	public get(player: Player): PlayerSession | undefined {
		return this.sessions.get(player);
	}

	public all(): Array<PlayerSession> {
		const result = new Array<PlayerSession>();
		for (const [, session] of this.sessions) {
			result.push(session);
		}
		return result;
	}

	public activeRunners(): Array<PlayerSession> {
		return this.all().filter((session) => session.tracker.getStatus() === RunStatus.Running);
	}

	public restart(player: Player, daily: boolean): PlayerSession | undefined {
		const session = this.sessions.get(player);
		if (!session) {
			return undefined;
		}
		session.tracker.abandon();
		session.respawnStage = 0;
		session.isDaily = daily;
		session.tracker.start(0);
		return session;
	}

	public finishRun(session: PlayerSession): void {
		session.tracker.finish();
		this.runFinished.fire(session);
	}

	public shutdown(): void {
		for (const [player] of this.sessions) {
			this.close(player);
		}
	}

	private open(player: Player): void {
		if (this.sessions.has(player)) {
			return;
		}
		const tracker = new RunTracker(player.UserId);
		const session: PlayerSession = {
			player,
			tracker,
			joinedAt: os.time(),
			respawnStage: 0,
			jumps: 0,
			isDaily: true,
		};
		tracker.splitRecorded.connect((split) => {
			session.respawnStage = split.stageIndex + 1;
			this.splitRecorded.fire(session, split);
		});
		player.CharacterAdded.Connect((character) => this.watchCharacter(session, character));
		this.sessions.set(player, session);
		tracker.start(0);
		this.sessionStarted.fire(session);
	}

	private watchCharacter(session: PlayerSession, character: Model): void {
		const humanoid = character.WaitForChild("Humanoid") as Humanoid;
		humanoid.StateChanged.Connect((_, state) => {
			if (state === Enum.HumanoidStateType.Jumping) {
				session.jumps += 1;
			}
		});
	}

	private close(player: Player): void {
		const session = this.sessions.get(player);
		if (!session) {
			return;
		}
		session.tracker.abandon();
		this.sessionEnded.fire(session);
		session.tracker.destroy();
		this.sessions.delete(player);
	}
}

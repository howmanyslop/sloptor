import { DataStoreService, Players, RunService } from "@rbxts/services";
import { LEADERBOARD_STORE_NAME } from "shared/config";
import { RemoteName, fireAllClients, fireClient } from "shared/network/remotes";
import type { LeaderboardEntry } from "shared/network/remotes";
import { bestDailyTime, recordDaily } from "shared/progress/progress-data";
import type { PlayerProgress } from "shared/progress/progress-data";
import { tryCall } from "shared/util/result";
import type { CourseService } from "./course-service";
import type { ProgressStore } from "./progress-store";
import { Service } from "./service";
import type { PlayerSession, SessionService } from "./session-service";

const REFRESH_INTERVAL = 60;
const TOP_COUNT = 10;
/** OrderedDataStore only stores integers, so times are stored in centiseconds. */
const TIME_SCALE = 100;

interface LeaderstatValues {
	readonly stage: IntValue;
	readonly deaths: IntValue;
	readonly best: StringValue;
}

export class LeaderboardService extends Service {
	private readonly leaderstats = new Map<Player, LeaderstatValues>();
	private readonly orderedStores = new Map<string, OrderedDataStore>();
	private cachedTop = new Array<LeaderboardEntry>();
	private running = true;

	public constructor(
		private readonly courses: CourseService,
		private readonly sessions: SessionService,
		private readonly progress: ProgressStore,
	) {
		super("LeaderboardService");
	}

	public init(): void {}

	public start(): void {
		this.progress.loaded.connect((player, data) => this.createLeaderstats(player, data));
		this.progress.changed.connect((player, data) => this.refreshLeaderstats(player, data));
		this.sessions.runFinished.connect((session) => this.onRunFinished(session));
		Players.PlayerRemoving.Connect((player) => this.leaderstats.delete(player));
		this.courses.courseReady.connect(() => task.spawn(() => this.refreshTop()));
		task.spawn(() => {
			while (this.running) {
				this.refreshTop();
				task.wait(REFRESH_INTERVAL);
			}
		});
	}

	public getTop(): ReadonlyArray<LeaderboardEntry> {
		return this.cachedTop;
	}

	public shutdown(): void {
		this.running = false;
	}

	private currentKey(): string | undefined {
		const mode = this.courses.getMode();
		return mode?.type === "daily" ? mode.info.key : undefined;
	}

	private storeFor(key: string): OrderedDataStore | undefined {
		if (RunService.IsStudio()) {
			return undefined;
		}
		const cached = this.orderedStores.get(key);
		if (cached) {
			return cached;
		}
		const result = tryCall(() => DataStoreService.GetOrderedDataStore(LEADERBOARD_STORE_NAME, key));
		if (!result.success) {
			warn(`[leaderboard] ordered store unavailable: ${result.error}`);
			return undefined;
		}
		this.orderedStores.set(key, result.value);
		return result.value;
	}

	private createLeaderstats(player: Player, data: PlayerProgress): void {
		const folder = new Instance("Folder");
		folder.Name = "leaderstats";
		const stage = new Instance("IntValue");
		stage.Name = "Stage";
		stage.Parent = folder;
		const deaths = new Instance("IntValue");
		deaths.Name = "Deaths";
		deaths.Parent = folder;
		const best = new Instance("StringValue");
		best.Name = "Best";
		best.Parent = folder;
		folder.Parent = player;
		this.leaderstats.set(player, { stage, deaths, best });
		this.refreshLeaderstats(player, data);
	}

	private refreshLeaderstats(player: Player, data: PlayerProgress): void {
		const values = this.leaderstats.get(player);
		if (!values) {
			return;
		}
		values.stage.Value = data.highestStage;
		values.deaths.Value = data.totalDeaths;
		const key = this.currentKey();
		const best = key !== undefined ? bestDailyTime(data, key) : undefined;
		values.best.Value = best !== undefined ? string.format("%.2fs", best) : "--";
	}

	private onRunFinished(session: PlayerSession): void {
		const { player, tracker } = session;
		const totalSeconds = tracker.totalSeconds();
		const key = this.currentKey();
		let newBest = false;
		if (key !== undefined && session.isDaily) {
			const previous = this.progress.get(player);
			const previousBest = previous ? bestDailyTime(previous, key) : undefined;
			newBest = previousBest === undefined || totalSeconds < previousBest;
			this.progress.update(player, (data) => recordDaily(data, key, totalSeconds));
			if (newBest) {
				this.submit(key, player.UserId, totalSeconds);
			}
		}
		const rank = this.cachedTop.findIndex((entry) => entry.userId === player.UserId);
		fireClient(player, RemoteName.RunFinished, {
			totalSeconds,
			deaths: tracker.totalDeaths(),
			rank: rank >= 0 ? rank + 1 : undefined,
			newBest,
		});
	}

	private submit(key: string, userId: number, seconds: number): void {
		const store = this.storeFor(key);
		if (!store) {
			return;
		}
		const scaled = math.floor(seconds * TIME_SCALE);
		task.spawn(() => {
			const result = tryCall(() =>
				store.UpdateAsync(tostring(userId), (old: number | undefined) => (old === undefined ? scaled : math.min(old, scaled))),
			);
			if (!result.success) {
				warn(`[leaderboard] submit failed: ${result.error}`);
			}
		});
	}

	private refreshTop(): void {
		const key = this.currentKey();
		const store = key !== undefined ? this.storeFor(key) : undefined;
		if (!store) {
			this.cachedTop = this.localTop();
			fireAllClients(RemoteName.LeaderboardUpdated, this.cachedTop);
			return;
		}
		const result = tryCall(() => store.GetSortedAsync(true, TOP_COUNT).GetCurrentPage());
		if (!result.success) {
			warn(`[leaderboard] refresh failed: ${result.error}`);
			return;
		}
		this.cachedTop = result.value.map(({ key: entryKey, value }) => {
			const userId = tonumber(entryKey) ?? 0;
			const player = Players.GetPlayerByUserId(userId);
			return {
				userId,
				displayName: player?.DisplayName ?? `#${userId}`,
				seconds: (value as number) / TIME_SCALE,
			};
		});
		fireAllClients(RemoteName.LeaderboardUpdated, this.cachedTop);
	}

	/** Studio fallback: rank whoever is in the server by their best daily time. */
	private localTop(): Array<LeaderboardEntry> {
		const key = this.currentKey();
		if (key === undefined) {
			return [];
		}
		const entries = Players.GetPlayers().mapFiltered((player): LeaderboardEntry | undefined => {
			const data = this.progress.get(player);
			const seconds = data ? bestDailyTime(data, key) : undefined;
			return seconds !== undefined ? { userId: player.UserId, displayName: player.DisplayName, seconds } : undefined;
		});
		const sorted = entries.sort((a, b) => a.seconds < b.seconds);
		const top = new Array<LeaderboardEntry>();
		for (const entry of sorted) {
			if (top.size() >= TOP_COUNT) {
				break;
			}
			top.push(entry);
		}
		return top;
	}
}

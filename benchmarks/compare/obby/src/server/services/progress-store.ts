import { DataStoreService, Players, RunService } from "@rbxts/services";
import { AUTOSAVE_INTERVAL, DATASTORE_NAME } from "shared/config";
import { defaultProgress, parseProgress } from "shared/progress/progress-data";
import type { PlayerProgress } from "shared/progress/progress-data";
import { retry } from "shared/util/result";
import { Signal } from "shared/util/signal";
import { Service } from "./service";

interface SessionEntry {
	progress: PlayerProgress;
	dirty: boolean;
	loadedAt: number;
	lastSavedAt: number;
}

type LoadState = "loading" | "ready" | "failed";

export class ProgressStore extends Service {
	public readonly loaded = new Signal<[player: Player, progress: PlayerProgress]>();
	public readonly changed = new Signal<[player: Player, progress: PlayerProgress]>();

	private store?: DataStore;
	private readonly sessions = new Map<number, SessionEntry>();
	private readonly states = new Map<number, LoadState>();
	private running = true;

	public constructor() {
		super("ProgressStore");
	}

	protected getPriority(): number {
		return 80;
	}

	public init(): void {
		const [success, result] = pcall(() => DataStoreService.GetDataStore(DATASTORE_NAME));
		if (success) {
			this.store = result;
		} else {
			warn(`[progress] DataStore unavailable, using session-only storage: ${result}`);
		}
	}

	public start(): void {
		Players.PlayerAdded.Connect((player) => this.load(player));
		Players.PlayerRemoving.Connect((player) => this.release(player));
		for (const player of Players.GetPlayers()) {
			task.spawn(() => this.load(player));
		}
		task.spawn(() => {
			while (this.running) {
				task.wait(AUTOSAVE_INTERVAL);
				this.saveAll(false);
			}
		});
	}

	public get(player: Player): PlayerProgress | undefined {
		return this.sessions.get(player.UserId)?.progress;
	}

	public getState(player: Player): LoadState | undefined {
		return this.states.get(player.UserId);
	}

	public update(player: Player, transform: (progress: PlayerProgress) => PlayerProgress): PlayerProgress | undefined {
		const entry = this.sessions.get(player.UserId);
		if (!entry) {
			return undefined;
		}
		const updated = transform(entry.progress);
		if (updated !== entry.progress) {
			entry.progress = updated;
			entry.dirty = true;
			this.changed.fire(player, updated);
		}
		return updated;
	}

	public shutdown(): void {
		this.running = false;
		this.saveAll(true);
	}

	private keyFor(userId: number): string {
		return `player_${userId}`;
	}

	private load(player: Player): void {
		const userId = player.UserId;
		if (this.states.has(userId)) {
			return;
		}
		this.states.set(userId, "loading");
		let progress = defaultProgress();
		const store = this.store;
		if (store && !RunService.IsStudio()) {
			const result = retry(3, 1, () => {
				const [value] = store.GetAsync(this.keyFor(userId));
				return value;
			});
			if (result.success) {
				progress = parseProgress(result.value) ?? defaultProgress();
			} else {
				warn(`[progress] load failed for ${player.Name}: ${result.error}`);
				this.states.set(userId, "failed");
			}
		}
		if (player.Parent === undefined) {
			this.states.delete(userId);
			return;
		}
		const now = os.clock();
		this.sessions.set(userId, { progress, dirty: false, loadedAt: now, lastSavedAt: now });
		if (this.states.get(userId) === "loading") {
			this.states.set(userId, "ready");
		}
		this.loaded.fire(player, progress);
	}

	private save(userId: number, force: boolean): boolean {
		const entry = this.sessions.get(userId);
		const store = this.store;
		if (!entry || !store || (!entry.dirty && !force)) {
			return true;
		}
		if (this.states.get(userId) === "failed") {
			// Never overwrite real data with defaults after a failed load.
			return false;
		}
		const snapshot = entry.progress;
		const result = retry(3, 2, () => {
			store.UpdateAsync(this.keyFor(userId), () => $tuple(snapshot));
		});
		if (result.success) {
			entry.dirty = false;
			entry.lastSavedAt = os.clock();
			return true;
		}
		warn(`[progress] save failed for ${userId}: ${result.error}`);
		return false;
	}

	private saveAll(force: boolean): void {
		const pending = new Array<number>();
		for (const [userId, entry] of this.sessions) {
			if (entry.dirty || force) {
				pending.push(userId);
			}
		}
		let remaining = pending.size();
		for (const userId of pending) {
			task.spawn(() => {
				this.save(userId, force);
				remaining -= 1;
			});
		}
		if (force) {
			const deadline = os.clock() + 25;
			while (remaining > 0 && os.clock() < deadline) {
				task.wait(0.1);
			}
		}
	}

	private release(player: Player): void {
		const userId = player.UserId;
		this.save(userId, false);
		this.sessions.delete(userId);
		this.states.delete(userId);
	}
}

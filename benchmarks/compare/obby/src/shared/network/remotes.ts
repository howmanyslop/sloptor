import { ReplicatedStorage, RunService } from "@rbxts/services";
import { REMOTES_FOLDER_NAME } from "../config";
import type { DifficultyTier } from "../types";

export enum RemoteName {
	CourseAnnounced = "CourseAnnounced",
	StageReached = "StageReached",
	RunFinished = "RunFinished",
	PlayerDied = "PlayerDied",
	RequestRespawn = "RequestRespawn",
	RequestRestart = "RequestRestart",
	UpdateSettings = "UpdateSettings",
	LeaderboardUpdated = "LeaderboardUpdated",
}

export interface CourseAnnouncement {
	readonly seed: number;
	readonly label: string;
	readonly stageCount: number;
	readonly themeName: string;
	readonly tiers: ReadonlyArray<DifficultyTier>;
	readonly secondsUntilReset: number;
}

export interface StageReachedPayload {
	readonly stageIndex: number;
	readonly splitSeconds: number;
	readonly personalBest?: number;
}

export interface RunFinishedPayload {
	readonly totalSeconds: number;
	readonly deaths: number;
	readonly rank?: number;
	readonly newBest: boolean;
}

export interface LeaderboardEntry {
	readonly userId: number;
	readonly displayName: string;
	readonly seconds: number;
}

/** Server -> client payloads. Each remote carries exactly one argument. */
export interface ServerToClient {
	[RemoteName.CourseAnnounced]: CourseAnnouncement;
	[RemoteName.StageReached]: StageReachedPayload;
	[RemoteName.RunFinished]: RunFinishedPayload;
	[RemoteName.PlayerDied]: { readonly cause: string; readonly stageIndex: number };
	[RemoteName.LeaderboardUpdated]: ReadonlyArray<LeaderboardEntry>;
}

export interface ClientToServer {
	[RemoteName.RequestRespawn]: undefined;
	[RemoteName.RequestRestart]: { readonly daily: boolean };
	[RemoteName.UpdateSettings]: { readonly key: string; readonly value: boolean };
}

function getFolder(): Folder {
	const existing = ReplicatedStorage.FindFirstChild(REMOTES_FOLDER_NAME);
	if (existing !== undefined && existing.IsA("Folder")) {
		return existing;
	}
	if (RunService.IsServer()) {
		const folder = new Instance("Folder");
		folder.Name = REMOTES_FOLDER_NAME;
		folder.Parent = ReplicatedStorage;
		return folder;
	}
	return ReplicatedStorage.WaitForChild(REMOTES_FOLDER_NAME) as Folder;
}

const cache = new Map<RemoteName, RemoteEvent>();

export function getRemote(name: RemoteName): RemoteEvent {
	const cached = cache.get(name);
	if (cached) {
		return cached;
	}
	const folder = getFolder();
	let remote = folder.FindFirstChild(name) as RemoteEvent | undefined;
	if (remote === undefined) {
		if (RunService.IsServer()) {
			remote = new Instance("RemoteEvent");
			remote.Name = name;
			remote.Parent = folder;
		} else {
			remote = folder.WaitForChild(name) as RemoteEvent;
		}
	}
	cache.set(name, remote);
	return remote;
}

export function fireClient<K extends keyof ServerToClient>(player: Player, name: K, payload: ServerToClient[K]): void {
	getRemote(name).FireClient(player, payload);
}

export function fireAllClients<K extends keyof ServerToClient>(name: K, payload: ServerToClient[K]): void {
	getRemote(name).FireAllClients(payload);
}

export function onClientEvent<K extends keyof ServerToClient>(
	name: K,
	handler: (payload: ServerToClient[K]) => void,
): RBXScriptConnection {
	return getRemote(name).OnClientEvent.Connect((payload: unknown) => handler(payload as ServerToClient[K]));
}

export function fireServer<K extends keyof ClientToServer>(name: K, payload: ClientToServer[K]): void {
	getRemote(name).FireServer(payload);
}

export function onServerEvent<K extends keyof ClientToServer>(
	name: K,
	validate: (raw: unknown) => raw is ClientToServer[K],
	handler: (player: Player, payload: ClientToServer[K]) => void,
): RBXScriptConnection {
	return getRemote(name).OnServerEvent.Connect((player, raw) => {
		if (validate(raw)) {
			handler(player, raw);
		} else {
			warn(`[remotes] rejected malformed ${name} from ${player.Name}`);
		}
	});
}

export function isRestartRequest(raw: unknown): raw is ClientToServer[RemoteName.RequestRestart] {
	return typeIs(raw, "table") && typeIs((raw as { daily?: unknown }).daily, "boolean");
}

export function isSettingsUpdate(raw: unknown): raw is ClientToServer[RemoteName.UpdateSettings] {
	if (!typeIs(raw, "table")) {
		return false;
	}
	const candidate = raw as { key?: unknown; value?: unknown };
	return typeIs(candidate.key, "string") && typeIs(candidate.value, "boolean");
}

export function isEmptyPayload(raw: unknown): raw is undefined {
	return raw === undefined;
}

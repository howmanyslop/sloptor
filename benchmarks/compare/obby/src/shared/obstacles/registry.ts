import { ObstacleKind, assertNever } from "../types";
import type { ObstacleSpec } from "../types";
import { Checkpoint } from "./checkpoint";
import { Conveyor } from "./conveyor";
import { DisappearingTiles } from "./disappearing-tiles";
import { KillBrick } from "./kill-brick";
import { Ladder } from "./ladder";
import { Laser } from "./laser";
import { MovingPlatform } from "./moving-platform";
import type { Obstacle } from "./obstacle";
import { StaticPlatform } from "./platform";
import { Spinner } from "./spinner";

export function createObstacle(spec: ObstacleSpec): Obstacle {
	const params = spec.params;
	switch (params.kind) {
		case ObstacleKind.Platform:
			return new StaticPlatform(spec, params);
		case ObstacleKind.MovingPlatform:
			return new MovingPlatform(spec, params);
		case ObstacleKind.Spinner:
			return new Spinner(spec, params);
		case ObstacleKind.Laser:
			return new Laser(spec, params);
		case ObstacleKind.DisappearingTiles:
			return new DisappearingTiles(spec, params);
		case ObstacleKind.KillBrick:
			return new KillBrick(spec, params);
		case ObstacleKind.Conveyor:
			return new Conveyor(spec, params);
		case ObstacleKind.Ladder:
			return new Ladder(spec, params);
		case ObstacleKind.Checkpoint:
			return new Checkpoint(spec, params);
		default:
			return assertNever(params, "createObstacle");
	}
}

export class ObstacleRegistry {
	private readonly byId = new Map<string, Obstacle>();
	private readonly byKind = new Map<ObstacleKind, Set<Obstacle>>();

	public register(obstacle: Obstacle): void {
		this.byId.set(obstacle.spec.id, obstacle);
		const kind = obstacle.getKind();
		let bucket = this.byKind.get(kind);
		if (bucket === undefined) {
			bucket = new Set();
			this.byKind.set(kind, bucket);
		}
		bucket.add(obstacle);
	}

	public unregister(id: string): Obstacle | undefined {
		const obstacle = this.byId.get(id);
		if (obstacle === undefined) {
			return undefined;
		}
		this.byId.delete(id);
		this.byKind.get(obstacle.getKind())?.delete(obstacle);
		return obstacle;
	}

	public get(id: string): Obstacle | undefined {
		return this.byId.get(id);
	}

	public ofKind<T extends Obstacle>(kind: ObstacleKind, guard: (obstacle: Obstacle) => obstacle is T): Array<T> {
		const result = new Array<T>();
		for (const obstacle of this.byKind.get(kind) ?? new Set<Obstacle>()) {
			if (guard(obstacle)) {
				result.push(obstacle);
			}
		}
		return result;
	}

	public all(): Array<Obstacle> {
		const result = new Array<Obstacle>();
		for (const [, obstacle] of this.byId) {
			result.push(obstacle);
		}
		return result;
	}

	public size(): number {
		return this.byId.size();
	}

	public countsByKind(): Map<ObstacleKind, number> {
		const counts = new Map<ObstacleKind, number>();
		for (const [kind, bucket] of this.byKind) {
			counts.set(kind, bucket.size());
		}
		return counts;
	}

	public stepAll(deltaTime: number): void {
		for (const [, obstacle] of this.byId) {
			obstacle.step(deltaTime);
		}
	}

	public clear(): void {
		for (const [, obstacle] of this.byId) {
			obstacle.destroy();
		}
		this.byId.clear();
		this.byKind.clear();
	}
}

export function isCheckpoint(obstacle: Obstacle): obstacle is Checkpoint {
	return obstacle instanceof Checkpoint;
}

export function isMovingPlatform(obstacle: Obstacle): obstacle is MovingPlatform {
	return obstacle instanceof MovingPlatform;
}

export function isConveyor(obstacle: Obstacle): obstacle is Conveyor {
	return obstacle instanceof Conveyor;
}

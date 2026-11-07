import { ObstacleKind } from "../types";
import type { Bounds, ObstacleSpec, ParamsOf } from "../types";
import { Obstacle } from "./obstacle";
import type { BuildContext, HazardResponse } from "./obstacle";

type TileParams = ParamsOf<ObstacleKind.DisappearingTiles>;

const TILE_SIZE = 5;
const TILE_GAP = 1;

interface TileState {
	readonly part: Part;
	readonly offset: number;
	solid: boolean;
}

export class DisappearingTiles extends Obstacle<TileParams> {
	private readonly tiles = new Array<TileState>();

	public constructor(spec: ObstacleSpec, params: TileParams) {
		super(spec, params);
	}

	public getKind(): ObstacleKind {
		return ObstacleKind.DisappearingTiles;
	}

	public getLocalBounds(): Bounds {
		const pitch = TILE_SIZE + TILE_GAP;
		const halfX = (this.params.columns * pitch) / 2;
		return { min: new Vector3(-halfX, -0.5, 0), max: new Vector3(halfX, 0.5, this.params.rows * pitch) };
	}

	public getHazardResponse(): HazardResponse {
		return "none";
	}

	public getDifficultyWeight(): number {
		const { visibleTime, hiddenTime, rows } = this.params;
		return 0.3 + (hiddenTime / (visibleTime + hiddenTime)) * 1.4 + rows * 0.05;
	}

	public getSolidCount(): number {
		return this.tiles.filter((tile) => tile.solid).size();
	}

	protected buildParts(context: BuildContext, model: Model): void {
		const origin = this.originFor(context);
		const pitch = TILE_SIZE + TILE_GAP;
		const halfWidth = ((this.params.columns - 1) * pitch) / 2;
		for (let row = 0; row < this.params.rows; row++) {
			for (let column = 0; column < this.params.columns; column++) {
				const part = this.createPart(model, {
					name: `Tile_${row}_${column}`,
					size: new Vector3(TILE_SIZE, 1, TILE_SIZE),
					cframe: origin.mul(new CFrame(column * pitch - halfWidth, 0, row * pitch + TILE_SIZE / 2)),
					material: Enum.Material.Glass,
					color: context.accent,
					transparency: 0.1,
				});
				this.tiles.push({ part, offset: ((row + column) % 2) * 0.5, solid: true });
			}
		}
	}

	protected animate(time: number): void {
		const { visibleTime, hiddenTime } = this.params;
		const period = visibleTime + hiddenTime;
		for (const tile of this.tiles) {
			const localTime = (time + tile.offset * period) % period;
			const shouldBeSolid = localTime < visibleTime;
			if (shouldBeSolid === tile.solid) {
				const fadeStart = visibleTime - 0.5;
				if (shouldBeSolid && localTime > fadeStart) {
					tile.part.Transparency = 0.1 + ((localTime - fadeStart) / 0.5) * 0.6;
				}
				continue;
			}
			tile.solid = shouldBeSolid;
			tile.part.CanCollide = shouldBeSolid;
			tile.part.Transparency = shouldBeSolid ? 0.1 : 0.95;
		}
	}
}

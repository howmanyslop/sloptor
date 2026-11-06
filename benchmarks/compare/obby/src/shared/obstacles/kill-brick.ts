import { ObstacleKind } from "../types";
import type { Bounds, ObstacleSpec, ParamsOf } from "../types";
import { Obstacle } from "./obstacle";
import type { BuildContext, HazardResponse } from "./obstacle";

type KillBrickParams = ParamsOf<ObstacleKind.KillBrick>;

const DANGER_COLOR = Color3.fromRGB(255, 30, 30);
const DIM_COLOR = Color3.fromRGB(120, 0, 0);

export class KillBrick extends Obstacle<KillBrickParams> {
	private brick?: Part;

	public constructor(spec: ObstacleSpec, params: KillBrickParams) {
		super(spec, params);
	}

	public getKind(): ObstacleKind {
		return ObstacleKind.KillBrick;
	}

	public getLocalBounds(): Bounds {
		const half = this.params.size.div(2);
		return { min: half.mul(-1), max: half };
	}

	public getHazardResponse(): HazardResponse {
		return "kill";
	}

	public getDifficultyWeight(): number {
		const { X, Y, Z } = this.params.size;
		return 0.4 + (X * Z) / 64 + (Y > 2 ? 0.3 : 0);
	}

	protected buildParts(context: BuildContext, model: Model): void {
		this.brick = this.createPart(model, {
			name: "Lava",
			size: this.params.size,
			cframe: this.originFor(context),
			material: Enum.Material.Neon,
			color: DANGER_COLOR,
		});
		this.tagHazard(this.brick);
		const light = new Instance("PointLight");
		light.Color = DANGER_COLOR;
		light.Range = math.max(this.params.size.X, this.params.size.Z);
		light.Brightness = 1.5;
		light.Parent = this.brick;
	}

	protected animate(time: number): void {
		if (!this.params.pulse || !this.brick) {
			return;
		}
		const alpha = (math.sin(time * 4) + 1) / 2;
		this.brick.Color = DIM_COLOR.Lerp(DANGER_COLOR, alpha);
	}
}

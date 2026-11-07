import { ObstacleKind } from "../types";
import type { Bounds, ObstacleSpec, ParamsOf } from "../types";
import { Obstacle } from "./obstacle";
import type { BuildContext, HazardResponse } from "./obstacle";

type ConveyorParams = ParamsOf<ObstacleKind.Conveyor>;

const BELT_WIDTH = 6;

export class Conveyor extends Obstacle<ConveyorParams> {
	private texture?: Texture;

	public constructor(spec: ObstacleSpec, params: ConveyorParams) {
		super(spec, params);
	}

	public getKind(): ObstacleKind {
		return ObstacleKind.Conveyor;
	}

	public getLocalBounds(): Bounds {
		return {
			min: new Vector3(-BELT_WIDTH / 2, -0.5, 0),
			max: new Vector3(BELT_WIDTH / 2, 0.5, this.params.length),
		};
	}

	public getHazardResponse(): HazardResponse {
		return "none";
	}

	public getDifficultyWeight(): number {
		// Negative speed pushes the player backwards, which is much harder.
		const against = this.params.speed < 0 ? 2 : 1;
		return 0.2 + (math.abs(this.params.speed) / 20) * against;
	}

	public getBeltVelocity(): Vector3 {
		return new Vector3(0, 0, this.params.speed);
	}

	protected buildParts(context: BuildContext, model: Model): void {
		const origin = this.originFor(context);
		const belt = this.createPart(model, {
			name: "Belt",
			size: new Vector3(BELT_WIDTH, 1, this.params.length),
			cframe: origin.mul(new CFrame(0, 0, this.params.length / 2)),
			material: Enum.Material.Fabric,
			color: Color3.fromRGB(35, 35, 40),
		});
		belt.AssemblyLinearVelocity = belt.CFrame.LookVector.mul(this.params.speed);

		const texture = new Instance("Texture");
		texture.Face = Enum.NormalId.Top;
		texture.StudsPerTileU = BELT_WIDTH;
		texture.StudsPerTileV = 4;
		texture.Color3 = context.accent;
		texture.Parent = belt;
		this.texture = texture;

		for (const side of [-1, 1]) {
			this.createPart(model, {
				name: side < 0 ? "RailLeft" : "RailRight",
				size: new Vector3(0.5, 1.5, this.params.length),
				cframe: origin.mul(new CFrame((side * (BELT_WIDTH + 0.5)) / 2, 0.25, this.params.length / 2)),
				material: Enum.Material.Metal,
				color: context.accent,
			});
		}
	}

	protected animate(_time: number, deltaTime: number): void {
		if (this.texture) {
			this.texture.OffsetStudsV = (this.texture.OffsetStudsV + this.params.speed * deltaTime) % 4;
		}
	}
}

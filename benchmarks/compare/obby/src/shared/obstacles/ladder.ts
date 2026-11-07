import { ObstacleKind } from "../types";
import type { Bounds, ObstacleSpec, ParamsOf } from "../types";
import { Obstacle } from "./obstacle";
import type { BuildContext, HazardResponse } from "./obstacle";

type LadderParams = ParamsOf<ObstacleKind.Ladder>;

const LADDER_WIDTH = 4;

export class Ladder extends Obstacle<LadderParams> {
	public constructor(spec: ObstacleSpec, params: LadderParams) {
		super(spec, params);
	}

	public getKind(): ObstacleKind {
		return ObstacleKind.Ladder;
	}

	public getLocalBounds(): Bounds {
		return {
			min: new Vector3(-LADDER_WIDTH / 2, 0, -1),
			max: new Vector3(LADDER_WIDTH / 2, this.params.height, 1),
		};
	}

	public getHazardResponse(): HazardResponse {
		return "none";
	}

	public getDifficultyWeight(): number {
		return 0.15 + this.params.height / 80;
	}

	public getRungCount(): number {
		return math.max(1, math.floor(this.params.height / this.params.rungSpacing));
	}

	protected buildParts(context: BuildContext, model: Model): void {
		const origin = this.originFor(context);
		const { height } = this.params;
		for (const side of [-1, 1]) {
			this.createPart(model, {
				name: side < 0 ? "RailLeft" : "RailRight",
				size: new Vector3(0.4, height, 0.4),
				cframe: origin.mul(new CFrame((side * LADDER_WIDTH) / 2, height / 2, 0)),
				material: Enum.Material.Wood,
				color: Color3.fromRGB(140, 100, 60),
			});
		}
		const rungs = this.getRungCount();
		for (let rung = 0; rung < rungs; rung++) {
			this.createPart(model, {
				name: `Rung${rung + 1}`,
				size: new Vector3(LADDER_WIDTH, 0.3, 0.6),
				cframe: origin.mul(new CFrame(0, (rung + 0.5) * this.params.rungSpacing, 0)),
				material: Enum.Material.Wood,
				color: context.accent.Lerp(Color3.fromRGB(140, 100, 60), 0.6),
			});
		}
		const truss = new Instance("TrussPart");
		truss.Name = "ClimbVolume";
		truss.Size = new Vector3(2, height, 2);
		truss.CFrame = origin.mul(new CFrame(0, height / 2, -1));
		truss.Transparency = 1;
		truss.Anchored = true;
		truss.Parent = model;
	}
}

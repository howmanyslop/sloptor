import { ObstacleKind } from "../types";
import type { Bounds, ObstacleSpec, ParamsOf } from "../types";
import { Obstacle } from "./obstacle";
import type { BuildContext, HazardResponse } from "./obstacle";

export class StaticPlatform extends Obstacle<ParamsOf<ObstacleKind.Platform>> {
	public constructor(spec: ObstacleSpec, params: ParamsOf<ObstacleKind.Platform>) {
		super(spec, params);
	}

	public getKind(): ObstacleKind {
		return ObstacleKind.Platform;
	}

	public getLocalBounds(): Bounds {
		const half = this.params.size.div(2);
		return { min: half.mul(-1), max: half };
	}

	public getHazardResponse(): HazardResponse {
		return "none";
	}

	public getDifficultyWeight(): number {
		const { X, Z } = this.params.size;
		const area = X * Z;
		return math.clamp(16 / math.max(area, 1), 0.05, 1.5);
	}

	protected buildParts(context: BuildContext, model: Model): void {
		const { size, material } = this.params;
		const base = this.createPart(model, {
			name: "Surface",
			size,
			cframe: this.originFor(context),
			material,
			color: context.accent.Lerp(new Color3(1, 1, 1), 0.35),
		});
		if (size.X >= 6 && size.Z >= 6) {
			const trim = new Instance("SelectionBox");
			trim.Adornee = base;
			trim.LineThickness = 0.05;
			trim.Color3 = context.accent;
			trim.Parent = base;
		}
	}
}

import { ObstacleKind } from "../types";
import type { Bounds, ObstacleSpec, ParamsOf } from "../types";
import { smoothstep, vectorMax, vectorMin } from "../util/math";
import { Obstacle } from "./obstacle";
import type { BuildContext, HazardResponse } from "./obstacle";

type MovingParams = ParamsOf<ObstacleKind.MovingPlatform>;

export class MovingPlatform extends Obstacle<MovingParams> {
	private platform?: Part;
	private anchor = new CFrame();
	private lastOffset = Vector3.zero;

	public constructor(spec: ObstacleSpec, params: MovingParams) {
		super(spec, params);
	}

	public getKind(): ObstacleKind {
		return ObstacleKind.MovingPlatform;
	}

	public getLocalBounds(): Bounds {
		const half = this.params.size.div(2);
		const start = half.mul(-1);
		const finish = this.params.travel.add(half);
		return { min: vectorMin(start, this.params.travel.sub(half)), max: vectorMax(half, finish) };
	}

	public getHazardResponse(): HazardResponse {
		return "none";
	}

	public getDifficultyWeight(): number {
		const speed = this.params.travel.Magnitude / math.max(this.params.period, 0.1);
		return 0.4 + math.min(speed / 12, 1.2);
	}

	/** Platform velocity at the last step, used to carry riders. */
	public getVelocity(): Vector3 {
		const deltaTime = 1 / 60;
		const current = this.offsetAt(this.elapsed);
		const previous = this.offsetAt(this.elapsed - deltaTime);
		return current.sub(previous).div(deltaTime);
	}

	protected buildParts(context: BuildContext, model: Model): void {
		this.anchor = this.originFor(context);
		this.platform = this.createPart(model, {
			name: "Mover",
			size: this.params.size,
			cframe: this.anchor,
			material: Enum.Material.Metal,
			color: context.accent,
		});
		const startMarker = new Instance("Attachment");
		startMarker.Name = "PathStart";
		startMarker.Parent = this.platform;
		const endMarker = new Instance("Attachment");
		endMarker.Name = "PathEnd";
		endMarker.Position = this.params.travel;
		endMarker.Parent = this.platform;
	}

	protected animate(time: number): void {
		const platform = this.platform;
		if (!platform) {
			return;
		}
		const offset = this.offsetAt(time);
		if (offset.sub(this.lastOffset).Magnitude < 1e-3) {
			return;
		}
		this.lastOffset = offset;
		platform.CFrame = this.anchor.add(this.anchor.VectorToWorldSpace(offset));
	}

	private offsetAt(time: number): Vector3 {
		const { period, phase, travel } = this.params;
		const cycle = ((time / period + phase) % 1) * 2;
		const alpha = cycle <= 1 ? smoothstep(0, 1, cycle) : smoothstep(0, 1, 2 - cycle);
		return travel.mul(alpha);
	}
}

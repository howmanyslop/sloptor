import { ObstacleKind } from "../types";
import type { Bounds, ObstacleSpec, ParamsOf } from "../types";
import { Obstacle } from "./obstacle";
import type { BuildContext, HazardResponse } from "./obstacle";

type SpinnerParams = ParamsOf<ObstacleKind.Spinner>;

const ARM_THICKNESS = 1.2;
const HUB_RADIUS = 2;

export class Spinner extends Obstacle<SpinnerParams> {
	private hub?: Part;
	private readonly arms = new Array<Part>();
	private pivot = new CFrame();

	public constructor(spec: ObstacleSpec, params: SpinnerParams) {
		super(spec, params);
	}

	public getKind(): ObstacleKind {
		return ObstacleKind.Spinner;
	}

	public getLocalBounds(): Bounds {
		const reach = this.params.armLength + HUB_RADIUS;
		return { min: new Vector3(-reach, -1, -reach), max: new Vector3(reach, 4, reach) };
	}

	public getHazardResponse(): HazardResponse {
		return "push";
	}

	public getDifficultyWeight(): number {
		const tipSpeed = (this.params.rpm / 60) * 2 * math.pi * this.params.armLength;
		return 0.6 + this.params.armCount * 0.15 + tipSpeed / 40;
	}

	public getAngle(time: number): number {
		return ((time * this.params.rpm) / 60) * 2 * math.pi;
	}

	protected buildParts(context: BuildContext, model: Model): void {
		this.pivot = this.originFor(context).add(new Vector3(0, 2, 0));
		this.hub = this.createPart(model, {
			name: "Hub",
			size: new Vector3(HUB_RADIUS * 2, 2, HUB_RADIUS * 2),
			cframe: this.pivot.mul(CFrame.Angles(0, 0, math.pi / 2)),
			shape: Enum.PartType.Cylinder,
			material: Enum.Material.DiamondPlate,
			color: Color3.fromRGB(60, 60, 70),
		});
		for (let index = 0; index < this.params.armCount; index++) {
			const arm = this.createPart(model, {
				name: `Arm${index + 1}`,
				size: new Vector3(this.params.armLength, ARM_THICKNESS, ARM_THICKNESS),
				cframe: this.armCFrame(index, 0),
				material: Enum.Material.Neon,
				color: context.accent,
			});
			this.tagHazard(arm);
			this.arms.push(arm);
		}
	}

	protected animate(time: number): void {
		const angle = this.getAngle(time);
		this.arms.forEach((arm, index) => {
			arm.CFrame = this.armCFrame(index, angle);
		});
		if (this.hub) {
			this.hub.CFrame = this.pivot.mul(CFrame.Angles(0, angle, math.pi / 2));
		}
	}

	private armCFrame(index: number, angle: number): CFrame {
		const armAngle = angle + (index / this.params.armCount) * 2 * math.pi;
		const offset = HUB_RADIUS + this.params.armLength / 2;
		return this.pivot.mul(CFrame.Angles(0, armAngle, 0)).mul(new CFrame(offset, 0, 0));
	}
}

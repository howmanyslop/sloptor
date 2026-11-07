import { ObstacleKind } from "../types";
import type { Bounds, ObstacleSpec, ParamsOf } from "../types";
import { Obstacle } from "./obstacle";
import type { BuildContext, HazardResponse } from "./obstacle";

type LaserParams = ParamsOf<ObstacleKind.Laser>;

export const enum LaserPhase {
	Off,
	Warning,
	On,
}

const WARNING_WINDOW = 0.6;

export class Laser extends Obstacle<LaserParams> {
	private beamPart?: Part;
	private beam?: Beam;
	private phase = LaserPhase.Off;

	public constructor(spec: ObstacleSpec, params: LaserParams) {
		super(spec, params);
	}

	public getKind(): ObstacleKind {
		return ObstacleKind.Laser;
	}

	public getLocalBounds(): Bounds {
		const half = this.params.span / 2;
		return { min: new Vector3(-half, 0, -0.5), max: new Vector3(half, 6, 0.5) };
	}

	public getHazardResponse(): HazardResponse {
		return "kill";
	}

	public getDifficultyWeight(): number {
		const dutyCycle = this.params.onTime / (this.params.onTime + this.params.offTime);
		return 0.5 + dutyCycle * 1.5;
	}

	public getPhase(): LaserPhase {
		return this.phase;
	}

	public phaseAt(time: number): LaserPhase {
		const { onTime, offTime } = this.params;
		const localTime = time % (onTime + offTime);
		if (localTime < offTime - WARNING_WINDOW) {
			return LaserPhase.Off;
		}
		return localTime < offTime ? LaserPhase.Warning : LaserPhase.On;
	}

	protected buildParts(context: BuildContext, model: Model): void {
		const origin = this.originFor(context);
		const half = this.params.span / 2;
		const postSize = new Vector3(1, 6, 1);
		const left = this.createPart(model, {
			name: "EmitterLeft",
			size: postSize,
			cframe: origin.mul(new CFrame(-half, 3, 0)),
			material: Enum.Material.Metal,
			color: Color3.fromRGB(40, 40, 40),
		});
		const right = this.createPart(model, {
			name: "EmitterRight",
			size: postSize,
			cframe: origin.mul(new CFrame(half, 3, 0)),
			material: Enum.Material.Metal,
			color: Color3.fromRGB(40, 40, 40),
		});
		const beamPart = this.createPart(model, {
			name: "Beam",
			size: new Vector3(this.params.span - 1, 0.4, 0.4),
			cframe: origin.mul(new CFrame(0, 2.5, 0)),
			material: Enum.Material.Neon,
			color: Color3.fromRGB(255, 40, 40),
			canCollide: false,
			transparency: 1,
		});
		this.tagHazard(beamPart);

		const startAttachment = new Instance("Attachment");
		startAttachment.Position = new Vector3(0, -0.5, 0);
		startAttachment.Parent = left;
		const endAttachment = new Instance("Attachment");
		endAttachment.Position = new Vector3(0, -0.5, 0);
		endAttachment.Parent = right;

		const beam = new Instance("Beam");
		beam.Attachment0 = startAttachment;
		beam.Attachment1 = endAttachment;
		beam.Width0 = 0.3;
		beam.Width1 = 0.3;
		beam.FaceCamera = true;
		beam.LightEmission = 1;
		beam.Enabled = false;
		beam.Parent = beamPart;

		this.beamPart = beamPart;
		this.beam = beam;
	}

	protected animate(time: number): void {
		const nextPhase = this.phaseAt(time);
		if (nextPhase === this.phase) {
			return;
		}
		this.phase = nextPhase;
		this.applyPhase(nextPhase);
	}

	private applyPhase(phase: LaserPhase): void {
		const beamPart = this.beamPart;
		const beam = this.beam;
		if (!beamPart || !beam) {
			return;
		}
		switch (phase) {
			case LaserPhase.Off:
				beam.Enabled = false;
				beamPart.Transparency = 1;
				beamPart.SetAttribute("HazardResponse", "none");
				break;
			case LaserPhase.Warning:
				beam.Enabled = true;
				beam.Color = new ColorSequence(Color3.fromRGB(255, 200, 60));
				beamPart.SetAttribute("HazardResponse", "none");
				break;
			case LaserPhase.On:
				beam.Enabled = true;
				beam.Color = new ColorSequence(Color3.fromRGB(255, 40, 40));
				beamPart.Transparency = 0.4;
				beamPart.SetAttribute("HazardResponse", "kill");
				break;
		}
	}
}

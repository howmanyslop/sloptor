import { ObstacleKind } from "../types";
import type { Bounds, ObstacleSpec, ParamsOf } from "../types";
import { Obstacle } from "./obstacle";
import type { BuildContext, HazardResponse } from "./obstacle";

type CheckpointParams = ParamsOf<ObstacleKind.Checkpoint>;

export const CHECKPOINT_ATTRIBUTE = "CheckpointStage";
const PAD_SIZE = new Vector3(10, 1, 10);

export class Checkpoint extends Obstacle<CheckpointParams> {
	private pad?: Part;
	private flag?: Part;

	public constructor(spec: ObstacleSpec, params: CheckpointParams) {
		super(spec, params);
	}

	public getKind(): ObstacleKind {
		return ObstacleKind.Checkpoint;
	}

	public getStageIndex(): number {
		return this.params.stageIndex;
	}

	public getLocalBounds(): Bounds {
		const half = PAD_SIZE.div(2);
		return { min: half.mul(-1), max: half.add(new Vector3(0, 8, 0)) };
	}

	public getHazardResponse(): HazardResponse {
		return "none";
	}

	public getDifficultyWeight(): number {
		return 0;
	}

	public getSpawnCFrame(): CFrame | undefined {
		return this.pad?.CFrame.add(new Vector3(0, 4, 0));
	}

	protected buildParts(context: BuildContext, model: Model): void {
		const origin = this.originFor(context);
		const pad = this.createPart(model, {
			name: "Pad",
			size: PAD_SIZE,
			cframe: origin,
			material: Enum.Material.Neon,
			color: context.accent,
		});
		pad.SetAttribute(CHECKPOINT_ATTRIBUTE, this.params.stageIndex);
		this.pad = pad;

		this.createPart(model, {
			name: "Pole",
			size: new Vector3(0.4, 8, 0.4),
			cframe: origin.mul(new CFrame(PAD_SIZE.X / 2 - 1, 4.5, PAD_SIZE.Z / 2 - 1)),
			material: Enum.Material.Metal,
		});
		this.flag = this.createPart(model, {
			name: "Flag",
			size: new Vector3(0.1, 2, 3),
			cframe: origin.mul(new CFrame(PAD_SIZE.X / 2 - 1, 7.5, PAD_SIZE.Z / 2 - 2.6)),
			material: Enum.Material.Fabric,
			color: Color3.fromRGB(230, 230, 230),
			canCollide: false,
		});

		const gui = new Instance("BillboardGui");
		gui.Size = UDim2.fromOffset(160, 40);
		gui.StudsOffset = new Vector3(0, 6, 0);
		gui.AlwaysOnTop = false;
		const label = new Instance("TextLabel");
		label.Size = UDim2.fromScale(1, 1);
		label.BackgroundTransparency = 1;
		label.Text = `Stage ${this.params.stageIndex + 1}`;
		label.TextScaled = true;
		label.TextColor3 = new Color3(1, 1, 1);
		label.Font = Enum.Font.GothamBold;
		label.Parent = gui;
		gui.Parent = pad;
	}

	protected animate(time: number): void {
		const flag = this.flag;
		const pad = this.pad;
		if (!flag || !pad) {
			return;
		}
		const wave = math.sin(time * 3) * 0.15;
		const base = pad.CFrame.mul(new CFrame(PAD_SIZE.X / 2 - 1, 7, PAD_SIZE.Z / 2 - 2.6));
		flag.CFrame = base.mul(CFrame.Angles(0, wave, 0));
	}

	public markReached(color: Color3): void {
		if (this.flag) {
			this.flag.Color = color;
		}
	}
}

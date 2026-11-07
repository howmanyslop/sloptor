import { Lighting, Players, TweenService, Workspace } from "@rbxts/services";
import { COURSE_FOLDER_NAME } from "shared/config";
import { RemoteName, onClientEvent } from "shared/network/remotes";
import { CHECKPOINT_ATTRIBUTE } from "shared/obstacles/checkpoint";
import { Controller } from "./controller";

const BURST_PARTICLES = 24;

export class CheckpointEffects extends Controller {
	private reducedMotion = false;
	private blur?: BlurEffect;

	public constructor() {
		super("CheckpointEffects");
	}

	public setReducedMotion(enabled: boolean): void {
		this.reducedMotion = enabled;
	}

	protected onEnable(): void {
		const blur = new Instance("BlurEffect");
		blur.Size = 0;
		blur.Parent = Lighting;
		this.blur = this.janitor.add(blur);

		this.janitor.add(onClientEvent(RemoteName.StageReached, ({ stageIndex }) => this.celebrate(stageIndex)));
		this.janitor.add(onClientEvent(RemoteName.PlayerDied, () => this.deathPulse()));
	}

	private findPad(stageIndex: number): BasePart | undefined {
		const root = Workspace.FindFirstChild(COURSE_FOLDER_NAME);
		if (!root) {
			return undefined;
		}
		for (const descendant of root.GetDescendants()) {
			if (descendant.IsA("BasePart") && descendant.GetAttribute(CHECKPOINT_ATTRIBUTE) === stageIndex) {
				return descendant;
			}
		}
		return undefined;
	}

	private celebrate(stageIndex: number): void {
		const pad = this.findPad(stageIndex);
		if (pad) {
			const original = pad.Color;
			const flash = TweenService.Create(pad, new TweenInfo(0.15, Enum.EasingStyle.Quad, Enum.EasingDirection.Out, 0, true), {
				Color: new Color3(1, 1, 1),
			});
			flash.Completed.Once(() => (pad.Color = original));
			flash.Play();
			if (!this.reducedMotion) {
				this.burst(pad);
			}
		}
		const camera = Workspace.CurrentCamera;
		if (camera && !this.reducedMotion) {
			const baseFov = camera.FieldOfView;
			const punch = TweenService.Create(camera, new TweenInfo(0.2, Enum.EasingStyle.Back, Enum.EasingDirection.Out, 0, true), {
				FieldOfView: baseFov + 6,
			});
			punch.Play();
		}
	}

	private burst(pad: BasePart): void {
		const attachment = new Instance("Attachment");
		attachment.Position = new Vector3(0, pad.Size.Y / 2 + 0.5, 0);
		attachment.Parent = pad;
		const emitter = new Instance("ParticleEmitter");
		emitter.Rate = 0;
		emitter.Speed = new NumberRange(12, 22);
		emitter.SpreadAngle = new Vector2(60, 60);
		emitter.Lifetime = new NumberRange(0.6, 1.1);
		emitter.Color = new ColorSequence(pad.Color, new Color3(1, 1, 1));
		emitter.Size = new NumberSequence([new NumberSequenceKeypoint(0, 0.6), new NumberSequenceKeypoint(1, 0)]);
		emitter.Parent = attachment;
		emitter.Emit(BURST_PARTICLES);
		task.delay(1.5, () => attachment.Destroy());
	}

	private deathPulse(): void {
		const blur = this.blur;
		if (!blur || this.reducedMotion) {
			return;
		}
		blur.Size = 16;
		TweenService.Create(blur, new TweenInfo(0.8), { Size: 0 }).Play();
		const character = Players.LocalPlayer.Character;
		const humanoid = character?.FindFirstChildOfClass("Humanoid");
		if (humanoid) {
			humanoid.CameraOffset = new Vector3(0, -1, 0);
			task.delay(0.3, () => (humanoid.CameraOffset = Vector3.zero));
		}
	}
}

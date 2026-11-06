import { CollectionService, RunService } from "@rbxts/services";
import { PLATFORM_TAG } from "../shared/constants";
import { readNumberAttribute } from "../shared/instance-factory";

interface PlatformState {
	readonly part: BasePart;
	readonly origin: CFrame;
	readonly speed: number;
	readonly amplitude: number;
	readonly phase: number;
}

export class PlatformAnimator {
	private readonly states = new Map<BasePart, PlatformState>();
	private connection?: RBXScriptConnection;
	private elapsed = 0;

	public start(): void {
		for (const instance of CollectionService.GetTagged(PLATFORM_TAG)) this.register(instance);
		CollectionService.GetInstanceAddedSignal(PLATFORM_TAG).Connect((instance) => this.register(instance));
		CollectionService.GetInstanceRemovedSignal(PLATFORM_TAG).Connect((instance) => {
			if (instance.IsA("BasePart")) this.states.delete(instance);
		});
		this.connection = RunService.Heartbeat.Connect((dt) => this.step(dt));
	}

	public stop(): void {
		this.connection?.Disconnect();
		this.connection = undefined;
		for (const [part, state] of this.states) part.CFrame = state.origin;
	}

	private register(instance: Instance): void {
		if (!instance.IsA("BasePart") || this.states.has(instance)) return;
		this.states.set(instance, {
			part: instance,
			origin: instance.CFrame,
			speed: readNumberAttribute(instance, "Speed", 1),
			amplitude: readNumberAttribute(instance, "Amplitude", 6),
			phase: readNumberAttribute(instance, "Phase", 0),
		});
	}

	private step(dt: number): void {
		this.elapsed += dt;
		for (const [, state] of this.states) {
			const t = this.elapsed * state.speed + state.phase;
			const offset = new Vector3(math.sin(t) * state.amplitude, math.sin(t * 2) * 2, math.cos(t) * state.amplitude * 0.5);
			const spin = CFrame.Angles(0, t * 0.25, 0);
			state.part.CFrame = state.origin.add(offset).mul(spin);
		}
	}

	public count(): number {
		return this.states.size();
	}
}

export class DayNightCycle {
	private connection?: RBXScriptConnection;

	public constructor(
		private readonly lighting: Lighting,
		private readonly minutesPerSecond: number,
	) {}

	public start(): void {
		this.connection = RunService.Heartbeat.Connect((dt) => {
			const minutes = (this.lighting.GetMinutesAfterMidnight() + dt * this.minutesPerSecond) % (24 * 60);
			this.lighting.SetMinutesAfterMidnight(minutes);
			const daylight = math.clamp(math.sin(((minutes / 60 - 6) / 12) * math.pi), 0, 1);
			this.lighting.Brightness = 0.5 + daylight * 2.5;
			this.lighting.OutdoorAmbient = Color3.fromRGB(40, 40, 70).Lerp(Color3.fromRGB(160, 160, 160), daylight);
			this.lighting.FogEnd = 300 + daylight * 700;
		});
	}

	public stop(): void {
		this.connection?.Disconnect();
	}
}

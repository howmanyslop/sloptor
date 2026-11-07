import { RunService, UserInputService, Workspace } from "@rbxts/services";
import { lerp, orbit, springStep } from "../shared/math";

export class OrbitCamera {
	private angle = 0;
	private pitch = 0.4;
	private distance = 30;
	private targetDistance = 30;
	private velocity = 0;
	private connections = new Array<RBXScriptConnection>();

	public constructor(private readonly subject: () => Vector3 | undefined) {}

	public enable(): void {
		const camera = Workspace.CurrentCamera;
		if (!camera) return;
		camera.CameraType = Enum.CameraType.Scriptable;
		this.connections.push(
			UserInputService.InputChanged.Connect((input, processed) => {
				if (processed) return;
				if (input.UserInputType === Enum.UserInputType.MouseWheel) {
					this.targetDistance = math.clamp(this.targetDistance - input.Position.Z * 4, 10, 80);
				} else if (input.UserInputType === Enum.UserInputType.MouseMovement && UserInputService.IsMouseButtonPressed(Enum.UserInputType.MouseButton2)) {
					this.angle -= input.Delta.X * 0.01;
					this.pitch = math.clamp(this.pitch + input.Delta.Y * 0.01, -0.2, 1.3);
				}
			}),
		);
		this.connections.push(
			RunService.RenderStepped.Connect((dt) => {
				const focus = this.subject();
				if (!focus) return;
				const [distance, velocity] = springStep(this.distance, this.velocity, this.targetDistance, 120, 18, dt);
				this.distance = distance;
				this.velocity = velocity;
				const center = new CFrame(focus);
				const eye = orbit(center, this.distance * math.cos(this.pitch), this.angle, this.distance * math.sin(this.pitch));
				camera.CFrame = camera.CFrame.Lerp(CFrame.lookAt(eye.Position, focus), math.min(1, dt * 10));
				camera.FieldOfView = lerp(camera.FieldOfView, 60 + this.distance * 0.2, dt * 4);
			}),
		);
	}

	public disable(): void {
		for (const connection of this.connections) connection.Disconnect();
		this.connections.clear();
		const camera = Workspace.CurrentCamera;
		if (camera) camera.CameraType = Enum.CameraType.Custom;
	}
}

export class InputMap {
	private readonly bindings = new Map<Enum.KeyCode, Array<() => void>>();

	public constructor() {
		UserInputService.InputBegan.Connect((input, processed) => {
			if (processed || input.UserInputType !== Enum.UserInputType.Keyboard) return;
			for (const action of this.bindings.get(input.KeyCode) ?? []) action();
		});
	}

	public bind(key: Enum.KeyCode, action: () => void): () => void {
		let list = this.bindings.get(key);
		if (!list) {
			list = [];
			this.bindings.set(key, list);
		}
		list.push(action);
		const owned = list;
		return () => {
			const index = owned.indexOf(action);
			if (index >= 0) owned.remove(index);
		};
	}
}

export function screenToWorld(screen: Vector2, maxDistance: number): RaycastResult | undefined {
	const camera = Workspace.CurrentCamera;
	if (!camera) return undefined;
	const ray = camera.ViewportPointToRay(screen.X, screen.Y);
	return Workspace.Raycast(ray.Origin, ray.Direction.mul(maxDistance));
}

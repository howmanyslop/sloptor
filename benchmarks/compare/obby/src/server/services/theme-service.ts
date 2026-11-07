import { Lighting, TweenService } from "@rbxts/services";
import type { DailyTheme } from "shared/generation/daily-seed";
import type { CourseMode, CourseService } from "./course-service";
import { Service } from "./service";

const NEUTRAL_THEME: DailyTheme = {
	name: "Default",
	accent: Color3.fromRGB(255, 255, 255),
	skyTint: Color3.fromRGB(180, 200, 230),
};

export class ThemeService extends Service {
	private atmosphere?: Atmosphere;
	private colorCorrection?: ColorCorrectionEffect;
	private activeTheme: DailyTheme = NEUTRAL_THEME;

	public constructor(private readonly courses: CourseService) {
		super("ThemeService");
	}

	public init(): void {
		this.atmosphere = Lighting.FindFirstChildOfClass("Atmosphere") ?? this.create("Atmosphere");
		this.colorCorrection = Lighting.FindFirstChildOfClass("ColorCorrectionEffect") ?? this.create("ColorCorrectionEffect");
	}

	public start(): void {
		this.courses.courseReady.connect((_, mode) => this.apply(this.themeFor(mode)));
	}

	public getActiveTheme(): DailyTheme {
		return this.activeTheme;
	}

	private themeFor(mode: CourseMode): DailyTheme {
		return mode.type === "daily" ? mode.info.theme : NEUTRAL_THEME;
	}

	private apply(theme: DailyTheme): void {
		this.activeTheme = theme;
		const info = new TweenInfo(3, Enum.EasingStyle.Sine, Enum.EasingDirection.InOut);
		const [hue, saturation] = theme.skyTint.ToHSV();
		const fogColor = Color3.fromHSV(hue, saturation * 0.6, 0.9);
		TweenService.Create(Lighting, info, { Ambient: theme.skyTint, OutdoorAmbient: fogColor }).Play();
		if (this.atmosphere) {
			TweenService.Create(this.atmosphere, info, { Color: theme.skyTint, Decay: theme.accent, Density: 0.3 }).Play();
		}
		if (this.colorCorrection) {
			TweenService.Create(this.colorCorrection, info, { TintColor: theme.skyTint.Lerp(new Color3(1, 1, 1), 0.8) }).Play();
		}
		print(`[theme] applied ${theme.name}`);
	}

	private create<T extends "Atmosphere" | "ColorCorrectionEffect">(className: T): CreatableInstances[T] {
		const instance = new Instance(className);
		instance.Parent = Lighting;
		return instance;
	}
}

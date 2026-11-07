import { dailyCourseFor, upcomingDailies } from "shared/generation/daily-seed";
import { formatDuration } from "shared/util/format";
import type { CourseService } from "./course-service";
import { Service } from "./service";
import type { SessionService } from "./session-service";

const CHECK_INTERVAL = 30;
const GRACE_SECONDS = 120;

/** Swaps the course at UTC midnight once nobody is mid-run (or after a grace period). */
export class DailyRotationService extends Service {
	private running = true;
	private pendingSince?: number;

	public constructor(
		private readonly courses: CourseService,
		private readonly sessions: SessionService,
	) {
		super("DailyRotationService");
	}

	public init(): void {
		const lines = new Array<string>();
		for (const daily of upcomingDailies(os.time(), 3)) {
			lines.push(`${daily.key} ${daily.theme.name} (${daily.stageCount} stages)`);
		}
		print(`[daily] upcoming: ${lines.join(", ")}`);
	}

	public start(): void {
		task.spawn(() => {
			while (this.running) {
				task.wait(CHECK_INTERVAL);
				this.tick(os.time());
			}
		});
	}

	public shutdown(): void {
		this.running = false;
	}

	private tick(now: number): void {
		const mode = this.courses.getMode();
		if (mode?.type !== "daily") {
			return;
		}
		const today = dailyCourseFor(now);
		if (today.key === mode.info.key) {
			this.pendingSince = undefined;
			return;
		}
		this.pendingSince ??= now;
		const runners = this.sessions.activeRunners().size();
		const waited = now - this.pendingSince;
		if (runners > 0 && waited < GRACE_SECONDS) {
			print(`[daily] rotation pending, ${runners} runners active (${formatDuration(GRACE_SECONDS - waited)} left)`);
			return;
		}
		print(`[daily] rotating ${mode.info.key} -> ${today.key}`);
		this.pendingSince = undefined;
		if (this.courses.loadDaily(now)) {
			for (const session of this.sessions.all()) {
				this.sessions.restart(session.player, true);
				session.player.LoadCharacter();
			}
		}
	}
}

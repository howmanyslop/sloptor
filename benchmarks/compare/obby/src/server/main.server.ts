import { Players } from "@rbxts/services";
import { CheckpointService } from "./services/checkpoint-service";
import { CourseService } from "./services/course-service";
import { DailyRotationService } from "./services/daily-rotation-service";
import { HazardService } from "./services/hazard-service";
import { LeaderboardService } from "./services/leaderboard-service";
import { ObstacleRuntime } from "./services/obstacle-runtime";
import { ProgressStore } from "./services/progress-store";
import { RemoteService } from "./services/remote-service";
import { Service } from "./services/service";
import { SessionService } from "./services/session-service";
import { StageBuilder } from "./services/stage-builder";
import { ThemeService } from "./services/theme-service";

Players.CharacterAutoLoads = true;

const courses = new CourseService();
const builder = new StageBuilder(courses);
const progress = new ProgressStore();
const sessions = new SessionService();
const checkpoints = new CheckpointService(courses, builder, sessions, progress);

new ObstacleRuntime(builder);
new HazardService(builder, sessions, checkpoints, progress);
new LeaderboardService(courses, sessions, progress);
new RemoteService(courses, sessions, checkpoints, progress);
new ThemeService(courses);
new DailyRotationService(courses, sessions);

Service.bootAll();
game.BindToClose(() => Service.shutdownAll());

print(`[obby] ${Service.getAll().size()} services booted`);

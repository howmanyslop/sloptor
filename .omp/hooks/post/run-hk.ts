import type { HookAPI } from "@oh-my-pi/pi-coding-agent/extensibility/hooks";

export default function runHk(hookApi: HookAPI): void {
	hookApi.on("turn_end", async () => {
		const result = await hookApi.exec("hk", ["run", "check", "--safe", "--format", "json"]);
		if (result.code !== 0) {
			hookApi.logger.error(`hk run check failed (exit ${result.code})`, { stderr: result.stderr });
		}
	});
}

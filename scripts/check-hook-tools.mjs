import { spawn } from "node:child_process";

const [tool, ...files] = process.argv.slice(2);

function run(command, args, captureOutput = false) {
	return new Promise((resolve) => {
		const child = spawn(command, args, {
			stdio: captureOutput ? ["ignore", "pipe", "inherit"] : "inherit",
		});
		let output = "";
		if (captureOutput) {
			child.stdout.setEncoding("utf8");
			child.stdout.on("data", (chunk) => {
				output += chunk;
			});
		}
		child.on("error", (error) => {
			console.error(error.message);
			resolve({ code: 1, output });
		});
		child.on("close", (code) => resolve({ code: code ?? 1, output }));
	});
}

if (tool === "gitleaks") {
	let next = 0;
	let failed = false;
	const workers = Array.from({ length: Math.min(8, files.length) }, async () => {
		while (next < files.length) {
			const file = files[next++];
			const { code } = await run("gitleaks", ["dir", "--redact", "--no-banner", "--log-level=error", file]);
			if (code !== 0) {
				console.error(`gitleaks failed: ${file}`);
				failed = true;
			}
		}
	});
	await Promise.all(workers);
	if (failed) process.exitCode = 1;
} else if (tool === "shfmt") {
	const { code, output } = await run("shfmt", ["-i", "4", "-l", "--apply-ignore", ...files], true);
	if (output) process.stdout.write(output);
	if (code !== 0 || output.trim()) process.exitCode = 1;
} else if (tool === "typos") {
	const { code, output } = await run("typos", ["--force-exclude", "--diff", ...files], true);
	if (output) process.stdout.write(output);
	if (code !== 0 || output.trim()) process.exitCode = 1;
} else {
	console.error(`Unknown hook tool: ${tool}`);
	process.exitCode = 1;
}

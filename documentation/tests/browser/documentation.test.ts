import { expect as playwrightExpect, test as playwrightIt } from "@playwright/test";

const baseUrl = "http://127.0.0.1:4321/sloptor/";

// Catches the home page losing its headline number or its way into the guide.
playwrightIt("leads the home page with the full-build time and a way in", async ({ page }) => {
	await page.goto(baseUrl);

	await playwrightExpect(page.locator(".overprint")).toHaveCount(1);
	await playwrightExpect(page.locator(".overprint")).toHaveText("355");
	await playwrightExpect(page.getByRole("link", { name: "Read the guide" })).toHaveAttribute(
		"href",
		"/sloptor/guide/introduction/",
	);
});

// Catches the command reference going missing or losing a command's flags.
playwrightIt("documents every flag of a command", async ({ page }) => {
	await page.goto(`${baseUrl}commands/build/`);

	await playwrightExpect(page.getByRole("heading", { level: 1 })).toHaveText("build");
	await playwrightExpect(page.getByRole("cell", { name: "-w, --watch" })).toBeVisible();
	await playwrightExpect(page.getByRole("cell", { name: "--checkers <n>" })).toBeVisible();
});

// Catches flags rendered with typographic dashes, which readers copy and the CLI
// rejects.
playwrightIt("keeps double hyphens in flag descriptions", async ({ page }) => {
	await page.goto(`${baseUrl}commands/build/`);

	await playwrightExpect(page.getByText("only with --build", { exact: false })).toBeVisible();
});

// Catches the header nav pointing nowhere or failing to show where the reader is.
playwrightIt("marks the current section in the header", async ({ page }) => {
	await page.goto(`${baseUrl}commands/deploy/`);

	const current = page.locator('.site-nav a[aria-current="page"]');
	await playwrightExpect(current).toHaveText("Commands");
	await playwrightExpect(current).toHaveAttribute("href", "/sloptor/commands/");
});

// Catches the deploy reference dropping the tables users name themselves.
playwrightIt("lists place settings under a named environment", async ({ page }) => {
	await page.goto(`${baseUrl}configuration/rotor-toml/`);

	await playwrightExpect(
		page.getByRole("heading", { name: "[deploy.environments.<environment>.places.<place>]" }),
	).toBeVisible();
	await playwrightExpect(page.getByRole("cell", { name: "placeId", exact: true })).toBeVisible();
});

// Catches "View as Markdown" serving an empty or HTML response.
playwrightIt("serves each page as Markdown", async ({ request }) => {
	const response = await request.get(`${baseUrl}guide/install/index.md`);

	playwrightExpect(response.ok()).toBe(true);
	playwrightExpect(response.headers()["content-type"]).toContain("text/markdown");
	playwrightExpect(await response.text()).toContain("@rotor-rbx/rotor");
});

// Catches the copy button copying nothing.
playwrightIt("copies a page as Markdown", async ({ page }) => {
	await page.context().grantPermissions(["clipboard-read", "clipboard-write"], { origin: baseUrl });
	await page.goto(`${baseUrl}guide/install/`);

	// The page actions show on hover where a pointer can hover.
	await page.getByRole("heading", { level: 1 }).hover();
	await page.getByRole("button", { name: "Copy page" }).click();
	await playwrightExpect(page.getByRole("button", { name: "Copied!" })).toBeVisible();
	playwrightExpect(await page.evaluate(async () => navigator.clipboard.readText())).toContain("## From npm");
});

// Catches the theme toggle leaving the page on the wrong palette.
playwrightIt("switches to the dark theme", async ({ page }) => {
	await page.goto(baseUrl);

	await page.locator("starlight-theme-select select").first().selectOption("dark");
	await playwrightExpect(page.locator("html")).toHaveAttribute("data-theme", "dark");
	await playwrightExpect(page.locator("body")).toHaveCSS("background-color", "rgb(17, 19, 21)");
});

// Catches the home page scrolling sideways on a phone.
playwrightIt("fits a phone screen without horizontal scroll", async ({ browser }) => {
	const context = await browser.newContext({
		hasTouch: true,
		isMobile: true,
		viewport: { height: 844, width: 390 },
	});
	const page = await context.newPage();
	await page.goto(baseUrl);

	playwrightExpect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(390);
	await context.close();
});

// Catches the search index being missing from the build.
playwrightIt("finds a command through search", async ({ page }) => {
	await page.goto(baseUrl);

	await page.getByRole("button", { name: "Search" }).click();
	await page.getByRole("dialog", { name: "Search" }).getByRole("textbox", { name: "Search" }).fill("sourcemap");
	// Pagefind falls back to the main thread when its worker is slow to start.
	await playwrightExpect(page.locator(".pagefind-ui__result-link").first()).toBeVisible({ timeout: 15_000 });
});

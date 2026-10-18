import { docsLoader } from "@astrojs/starlight/loaders";
import { docsSchema } from "@astrojs/starlight/schema";
import { glob } from "astro/loaders";
import { defineCollection } from "astro:content";

const documentation = defineCollection({
	loader: docsLoader(),
	schema: docsSchema(),
});

const releases = defineCollection({
	loader: glob({ base: "./src/content/releases", pattern: "*.md" }),
});

export const collections = { docs: documentation, releases };

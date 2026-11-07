import type { ComponentRenderer, MarkdownHeading } from "astro";
import type { UnknownRecord } from "type-fest";

interface Renderer {
	readonly Content: ComponentRenderer;
	readonly headings: Array<MarkdownHeading>;
	readonly remarkPluginFrontmatter: UnknownRecord;
}

declare module "astro:content" {
	interface CollectionEntry<TCollection extends string = string> {
		readonly id: string;
		readonly body: string;
		readonly collection: TCollection;
		readonly data: UnknownRecord;
		readonly render: () => Promise<Renderer>;
		readonly slug: string;
	}
}

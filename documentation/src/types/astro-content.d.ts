import type { ComponentRenderer, MarkdownHeading } from "astro";
import type { UnknownRecord } from "type-fest";

declare module "astro:content" {
	interface CollectionEntry<TCollection extends string = string> {
		body: string;
		collection: TCollection;
		data: UnknownRecord;
		id: string;
		render: () => Promise<{
			Content: ComponentRenderer;
			headings: Array<MarkdownHeading>;
			remarkPluginFrontmatter: UnknownRecord;
		}>;
		slug: string;
	}
}

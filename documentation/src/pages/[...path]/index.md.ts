import { getCollection } from "astro:content";

import { createMarkdownResponse } from "$utilities/create-markdown-response";

import type { APIRoute, GetStaticPaths, GetStaticPathsResult } from "astro";

interface MarkdownRouteProperties {
	readonly body: string;
}

function isMarkdownRouteProperties(value: unknown): value is MarkdownRouteProperties {
	return typeof value === "object" && value !== null && "body" in value && typeof value.body === "string";
}

export const getStaticPaths = (async (): Promise<GetStaticPathsResult> => {
	const entries = await getCollection("docs");
	const staticPaths: GetStaticPathsResult = [];
	let size = 0;

	for (const entry of entries) {
		if (entry.id.length === 0) continue;
		staticPaths[size++] = {
			params: { path: entry.id },
			// oxlint-disable-next-line small-rules/prevent-abbreviations -- bruh.
			props: { body: entry.body },
		};
	}

	return staticPaths;
}) satisfies GetStaticPaths;

// oxlint-disable-next-line small-rules/require-async-suffix typescript/require-await -- Astro route handlers must be named GET.
export const GET: APIRoute = async (context): Promise<Response> => {
	// oxlint-disable-next-line small-rules/prevent-abbreviations -- Astro route contexts expose this field as props.
	const routeProperties: unknown = context.props;
	if (!isMarkdownRouteProperties(routeProperties) || routeProperties.body.length === 0) {
		return createMarkdownResponse({ body: "", status: 404 });
	}

	return createMarkdownResponse({ body: routeProperties.body });
};

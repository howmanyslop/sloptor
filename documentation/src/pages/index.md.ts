import { createMarkdownResponse } from "$utilities/create-markdown-response";
import { getCollection } from "astro:content";

import type { APIRoute } from "astro";

// oxlint-disable-next-line small-rules/require-async-suffix -- Astro route handlers must be named GET.
export const GET: APIRoute = async (): Promise<Response> => {
	const entries = await getCollection("docs");
	const entry = entries.find(({ id }) => id.length === 0 || id === "index");

	const body = entry?.body;
	if (body === undefined || body.length === 0) {
		return createMarkdownResponse({ body: "", status: 404 });
	}

	return createMarkdownResponse({ body });
};

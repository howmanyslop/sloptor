export interface MarkdownResponseOptions {
	readonly body: string;
	readonly status?: number | undefined;
}

const MARKDOWN_CONTENT_TYPE = "text/markdown; charset=utf-8";

export function createMarkdownResponse(options: MarkdownResponseOptions): Response {
	if (!options.body || options.body.trim().length === 0) {
		return new Response("Not Found", {
			headers: { "Content-Type": "text/plain; charset=utf-8" },
			status: 404,
		});
	}

	return new Response(options.body, {
		headers: {
			"Cache-Control": "public, max-age=3600",
			"Content-Type": MARKDOWN_CONTENT_TYPE,
		},
		status: options.status ?? 200,
	});
}

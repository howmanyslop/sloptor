import { useSyncExternalStore } from "react";

const QUERY = "(prefers-reduced-motion: reduce)";

function getSnapshot(): boolean {
	return matchMedia(QUERY).matches;
}

function subscribe(callback: () => void): () => void {
	const query = matchMedia(QUERY);
	query.addEventListener("change", callback);
	return () => {
		query.removeEventListener("change", callback);
	};
}

function getServerSnapshot(): boolean {
	return false;
}

/**
 * Whether the visitor asked for less motion, kept current if they change it.
 *
 * @returns True when animation should be skipped.
 */
export function usePrefersReducedMotion(): boolean {
	return useSyncExternalStore(subscribe, getSnapshot, getServerSnapshot);
}

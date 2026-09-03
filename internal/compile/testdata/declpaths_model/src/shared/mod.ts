export interface Shared {
	label: string;
}

export const SHARED_VALUE = 5;

export function makeShared(): Shared {
	return { label: "shared" };
}

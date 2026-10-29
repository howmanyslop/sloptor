// Placeholder for WP8: the three generic aliases the translator must turn
// into Luau type functions (homomorphic mapped, distributive conditional with
// infer, key-remapped mapped over a template literal).
export type Partial2<T> = { [K in keyof T]?: T[K] };

export type Elem<T> = T extends (infer U)[] ? U : T;

export type Handlers<T extends string> = { [K in T as `on${K}`]: () => void };

export interface Config {
	host: string;
	port: number;
}

export function patch(config: Config, partial: Partial2<Config>): Config {
	return { host: partial.host ?? config.host, port: partial.port ?? config.port };
}

export function first(values: number[]): Elem<number[]> {
	return values[0];
}

export function handlers(): Handlers<"click" | "hover"> {
	return { onclick: () => {}, onhover: () => {} };
}

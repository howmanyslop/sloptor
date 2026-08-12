import { Networking } from "@flamework/networking";

interface ServerEvents {
	readonly submitted: (payload: { readonly id: string; readonly count: number }) => void;
}

interface ClientEvents {
	readonly accepted: (id: string) => void;
}

interface ServerFunctions {
	readonly lookup: (id: string) => { readonly count: number };
}

interface ClientFunctions {
	readonly confirm: (id: string) => boolean;
}

export const events = Networking.createEvent<ServerEvents, ClientEvents>();
export const functions = Networking.createFunction<ServerFunctions, ClientFunctions>();

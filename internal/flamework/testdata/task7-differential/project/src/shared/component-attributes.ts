import { BaseComponent, Component } from "@flamework/components";

interface Attributes {
	readonly active: boolean;
	readonly label: string;
	readonly retries: number;
}

@Component({ tag: "Task4Fixture" })
export class FixtureComponent extends BaseComponent<Attributes, Part> {}

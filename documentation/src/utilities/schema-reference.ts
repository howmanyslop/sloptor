export interface JsonSchema {
	readonly additionalProperties?: boolean | JsonSchema;
	readonly default?: unknown;
	readonly description?: string;
	readonly enum?: ReadonlyArray<unknown>;
	readonly items?: JsonSchema;
	readonly properties?: Readonly<Record<string, JsonSchema>>;
	readonly type?: string;
}

export interface SchemaKey {
	readonly defaultValue: string | undefined;
	readonly description: string;
	readonly name: string;
	readonly type: string;
}

export interface SchemaTable {
	readonly description: string;
	readonly keys: ReadonlyArray<SchemaKey>;
	readonly path: string;
}

function isTable(schema: JsonSchema): boolean {
	return schema.type === "object" || schema.properties !== undefined;
}

function singular(name: string): string {
	return name.endsWith("s") ? name.slice(0, -1) : name;
}

function describeType(schema: JsonSchema): string {
	if (schema.enum !== undefined) return schema.enum.map((value) => JSON.stringify(value)).join(" | ");
	if (schema.type === "array") {
		const item = schema.items?.type;
		return item === undefined ? "array" : `${item}[]`;
	}
	return schema.type ?? "any";
}

function collect(schema: JsonSchema, path: string, tables: Array<SchemaTable>): void {
	const keys = new Array<SchemaKey>();
	const children = new Array<[string, JsonSchema]>();

	for (const [name, property] of Object.entries(schema.properties ?? {})) {
		if (isTable(property)) {
			children.push([path.length === 0 ? name : `${path}.${name}`, property]);
			continue;
		}
		keys.push({
			defaultValue: property.default === undefined ? undefined : JSON.stringify(property.default),
			description: property.description ?? "",
			name,
			type: describeType(property),
		});
	}

	if (path.length > 0 && (keys.length > 0 || schema.description !== undefined)) {
		tables.push({ description: schema.description ?? "", keys, path });
	}

	const entry = schema.additionalProperties;
	if (typeof entry === "object" && isTable(entry)) {
		const segment = path.split(".").at(-1) ?? "entry";
		collect(entry, `${path}.<${singular(segment)}>`, tables);
	}

	for (const [childPath, child] of children) collect(child, childPath, tables);
}

/**
 * Flattens a JSON schema into TOML-style tables, one per object, in the order the schema declares them. Map-shaped
 * objects (`additionalProperties` with a schema) get a `<name>` placeholder segment, so `deploy.environments` becomes
 * `deploy.environments.<environment>`.
 *
 * @param schema - The root JSON schema.
 * @returns Every table that has keys or a description.
 */
export function schemaTables(schema: JsonSchema): ReadonlyArray<SchemaTable> {
	const tables = new Array<SchemaTable>();
	collect(schema, "", tables);
	return tables;
}

import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const LAYERS = 10;
const PER_LAYER = 40;
const HUBS = 4;

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "src");

let state = 20261106;
function random(): number {
	state = (state * 1103515245 + 12345) % 2147483648;
	return state / 2147483648;
}
function pick<T>(items: ReadonlyArray<T>): T {
	return items[Math.floor(random() * items.length)];
}
function integer(min: number, max: number): number {
	return min + Math.floor(random() * (max - min + 1));
}

interface ModuleInfo {
	layer: number;
	index: number;
	id: string;
	path: string;
}

const NOUNS = ["Widget", "Gadget", "Sprocket", "Ledger", "Beacon", "Lattice", "Vessel", "Prism", "Anchor", "Relay"];
const VERBS = ["blend", "fold", "scale", "shift", "mix", "weigh", "tally", "warp"];
const STATES = ["Idle", "Active", "Paused", "Done", "Failed", "Queued", "Locked"];

function moduleId(layer: number, index: number): string {
	return `L${layer}M${String(index).padStart(2, "0")}`;
}

function modulePath(layer: number, index: number): string {
	return `layer${layer}/module${String(index).padStart(2, "0")}.ts`;
}

function relativeImport(from: ModuleInfo, to: ModuleInfo): string {
	if (from.layer === to.layer) return `./module${to.path.split("module")[1].replace(".ts", "")}`;
	return `../${to.path.replace(".ts", "")}`;
}

function chooseDependencies(layer: number, index: number, modules: Array<Array<ModuleInfo>>): Array<ModuleInfo> {
	if (layer === 0) return [];
	const chosen = new Map<string, ModuleInfo>();
	const previous = modules[layer - 1];
	chosen.set("primary", previous[(index * 7 + layer) % previous.length]);
	const extra = integer(0, 2);
	for (let count = 0; count < extra; count++) {
		const sourceLayer = integer(0, layer - 1);
		const candidate = pick(modules[sourceLayer]);
		chosen.set(candidate.id, candidate);
	}
	if (random() < 0.35) {
		const hub = modules[0][integer(0, HUBS - 1)];
		chosen.set(hub.id, hub);
	}
	const unique = new Map<string, ModuleInfo>();
	for (const info of chosen.values()) unique.set(info.id, info);
	return [...unique.values()];
}

function renderModule(info: ModuleInfo, dependencies: Array<ModuleInfo>): string {
	const id = info.id;
	const noun = pick(NOUNS);
	const verb = pick(VERBS);
	const stateCount = integer(3, 5);
	const states = [...STATES].sort(() => random() - 0.5).slice(0, stateCount);
	const factor = integer(2, 19);
	const offset = integer(1, 97);
	const lines: Array<string> = [];

	for (const dependency of dependencies) {
		lines.push(`import { compute${dependency.id}, ${dependency.id}Kind, Base${dependency.id} } from "${relativeImport(info, dependency)}";`);
	}
	if (dependencies.length > 0) lines.push("");

	lines.push(`export enum ${id}Kind {`);
	for (const name of states) lines.push(`\t${name},`);
	lines.push("}");
	lines.push("");
	lines.push(`export const ${id}_FACTOR = ${factor};`);
	lines.push("");

	lines.push(`export class Base${id} {`);
	lines.push(`\tprotected total = ${offset};`);
	lines.push(`\tpublic kind = ${id}Kind.${states[0]};`);
	lines.push("");
	lines.push(`\tpublic ${verb}(value: number): number {`);
	lines.push(`\t\tthis.total += value * ${id}_FACTOR;`);
	lines.push(`\t\treturn this.total;`);
	lines.push("\t}");
	lines.push("");
	lines.push(`\tpublic advance(): ${id}Kind {`);
	lines.push(`\t\tconst order = [${states.map((name) => `${id}Kind.${name}`).join(", ")}];`);
	lines.push(`\t\tconst position = order.indexOf(this.kind);`);
	lines.push(`\t\tthis.kind = order[(position + 1) % order.size()];`);
	lines.push(`\t\treturn this.kind;`);
	lines.push("\t}");
	lines.push("");
	lines.push(`\tpublic read(): number {`);
	lines.push(`\t\treturn this.total;`);
	lines.push("\t}");
	lines.push("}");
	lines.push("");

	if (random() < 0.6) {
		const parent = dependencies.length > 0 && random() < 0.5 ? `Base${dependencies[0].id}` : `Base${id}`;
		lines.push(`export class ${noun}${id} extends ${parent} {`);
		lines.push(`\tprivate readonly history = new Array<number>();`);
		lines.push("");
		lines.push(`\tpublic record(value: number): void {`);
		lines.push(`\t\tthis.history.push(value);`);
		lines.push(`\t\tif (this.history.size() > ${integer(4, 16)}) this.history.shift();`);
		lines.push("\t}");
		lines.push("");
		lines.push(`\tpublic average(): number {`);
		lines.push(`\t\tlet sum = 0;`);
		lines.push(`\t\tfor (const value of this.history) sum += value;`);
		lines.push(`\t\treturn this.history.size() > 0 ? sum / this.history.size() : this.read();`);
		lines.push("\t}");
		lines.push("}");
		lines.push("");
	}

	lines.push(`export function compute${id}(input: number): number {`);
	lines.push(`\tlet result = input * ${factor} + ${offset};`);
	for (const dependency of dependencies) lines.push(`\tresult += compute${dependency.id}(input % ${integer(3, 11)});`);
	lines.push(`\treturn result % ${integer(1000, 99999)};`);
	lines.push("}");
	lines.push("");

	const style = integer(0, 2);
	if (style === 0) {
		lines.push(`export function describe${id}(kind: ${id}Kind): string {`);
		lines.push(`\tswitch (kind) {`);
		for (const name of states) lines.push(`\t\tcase ${id}Kind.${name}:\n\t\t\treturn "${name.toLowerCase()}";`);
		lines.push(`\t}`);
		lines.push("}");
	} else if (style === 1) {
		lines.push(`export function table${id}(size: number): Map<number, number> {`);
		lines.push(`\tconst lookup = new Map<number, number>();`);
		lines.push(`\tfor (let index = 0; index < size; index++) lookup.set(index, compute${id}(index));`);
		lines.push(`\treturn lookup;`);
		lines.push("}");
	} else {
		lines.push(`export const ${id}_HANDLERS: Record<string, (value: number) => number> = {`);
		lines.push(`\tdouble: (value) => value * 2,`);
		lines.push(`\toffset: (value) => value + ${offset},`);
		lines.push(`\tchain: (value) => compute${id}(value),`);
		lines.push("};");
	}
	lines.push("");

	if (dependencies.length > 0) {
		lines.push(`export function link${id}(): Array<number> {`);
		lines.push(`\tconst results = new Array<number>();`);
		for (const dependency of dependencies) {
			lines.push(`\tconst instance${dependency.id} = new Base${dependency.id}();`);
			lines.push(`\tinstance${dependency.id}.advance();`);
			lines.push(`\tresults.push(instance${dependency.id}.read() + ${dependency.id}Kind.${"__first__"});`);
		}
		lines.push(`\treturn results;`);
		lines.push("}");
		lines.push("");
	}

	return lines.join("\n");
}

const modules: Array<Array<ModuleInfo>> = [];
const firstStates = new Map<string, string>();
const rendered = new Map<string, string>();

rmSync(root, { recursive: true, force: true });
for (let layer = 0; layer < LAYERS; layer++) {
	const current: Array<ModuleInfo> = [];
	for (let index = 0; index < PER_LAYER; index++) {
		current.push({ layer, index, id: moduleId(layer, index), path: modulePath(layer, index) });
	}
	modules.push(current);
}

for (const layer of modules) {
	for (const info of layer) {
		const dependencies = chooseDependencies(info.layer, info.index, modules);
		const source = renderModule(info, dependencies);
		const firstState = source.match(new RegExp(`export enum ${info.id}Kind \\{\\n\\t(\\w+),`))![1];
		firstStates.set(info.id, firstState);
		rendered.set(info.path, source);
	}
}

let total = 0;
for (const [path, source] of rendered) {
	const resolved = source.replace(/(\w+)Kind\.__first__/g, (_, id: string) => `${id}Kind.${firstStates.get(id)}`);
	const file = join(root, path);
	mkdirSync(dirname(file), { recursive: true });
	writeFileSync(file, resolved);
	total++;
}

const mainLines: Array<string> = [];
const tops = modules[LAYERS - 1];
for (const info of tops) mainLines.push(`import { compute${info.id} } from "./${info.path.replace(".ts", "")}";`);
mainLines.push("");
mainLines.push("let checksum = 0;");
for (const info of tops) mainLines.push(`checksum += compute${info.id}(${info.index});`);
mainLines.push("print(`checksum ${checksum}`);");
mainLines.push("");
writeFileSync(join(root, "main.ts"), mainLines.join("\n"));
total++;

console.log(`generated ${total} files in ${root}`);

export interface Edge {
	readonly from: number;
	readonly to: number;
	readonly weight: number;
}

export class Graph {
	private readonly adjacency = new Map<number, Array<Edge>>();
	private edgeCount = 0;

	public constructor(public readonly directed: boolean) {}

	public addVertex(vertex: number): void {
		if (!this.adjacency.has(vertex)) this.adjacency.set(vertex, []);
	}

	public addEdge(from: number, to: number, weight = 1): void {
		this.addVertex(from);
		this.addVertex(to);
		this.adjacency.get(from)!.push({ from, to, weight });
		if (!this.directed) this.adjacency.get(to)!.push({ from: to, to: from, weight });
		this.edgeCount++;
	}

	public neighbors(vertex: number): ReadonlyArray<Edge> {
		return this.adjacency.get(vertex) ?? [];
	}

	public vertices(): Array<number> {
		const result = new Array<number>();
		for (const [vertex] of this.adjacency) result.push(vertex);
		result.sort((left, right) => left < right);
		return result;
	}

	public edges(): Array<Edge> {
		const result = new Array<Edge>();
		for (const [, list] of this.adjacency) for (const edge of list) result.push(edge);
		return result;
	}

	public vertexCount(): number {
		return this.adjacency.size();
	}

	public getEdgeCount(): number {
		return this.edgeCount;
	}

	public transpose(): Graph {
		const result = new Graph(this.directed);
		for (const vertex of this.vertices()) result.addVertex(vertex);
		for (const edge of this.edges()) result.addEdge(edge.to, edge.from, edge.weight);
		return result;
	}

	public static grid(width: number, height: number, blocked: ReadonlySet<number>): Graph {
		const graph = new Graph(false);
		const id = (x: number, y: number) => y * width + x;
		for (let y = 0; y < height; y++) {
			for (let x = 0; x < width; x++) {
				if (blocked.has(id(x, y))) continue;
				graph.addVertex(id(x, y));
				if (x + 1 < width && !blocked.has(id(x + 1, y))) graph.addEdge(id(x, y), id(x + 1, y));
				if (y + 1 < height && !blocked.has(id(x, y + 1))) graph.addEdge(id(x, y), id(x, y + 1));
			}
		}
		return graph;
	}
}

export class DisjointSet {
	private readonly parent = new Map<number, number>();
	private readonly rank = new Map<number, number>();

	public find(value: number): number {
		if (!this.parent.has(value)) {
			this.parent.set(value, value);
			this.rank.set(value, 0);
		}
		let root = value;
		while (this.parent.get(root) !== root) root = this.parent.get(root)!;
		let current = value;
		while (current !== root) {
			const following = this.parent.get(current)!;
			this.parent.set(current, root);
			current = following;
		}
		return root;
	}

	public union(left: number, right: number): boolean {
		const leftRoot = this.find(left);
		const rightRoot = this.find(right);
		if (leftRoot === rightRoot) return false;
		const leftRank = this.rank.get(leftRoot)!;
		const rightRank = this.rank.get(rightRoot)!;
		if (leftRank < rightRank) this.parent.set(leftRoot, rightRoot);
		else if (leftRank > rightRank) this.parent.set(rightRoot, leftRoot);
		else {
			this.parent.set(rightRoot, leftRoot);
			this.rank.set(leftRoot, leftRank + 1);
		}
		return true;
	}
}

export function kruskal(graph: Graph): Array<Edge> {
	const edges = graph.edges();
	edges.sort((left, right) => left.weight < right.weight);
	const set = new DisjointSet();
	const tree = new Array<Edge>();
	for (const edge of edges) {
		if (set.union(edge.from, edge.to)) tree.push(edge);
	}
	return tree;
}

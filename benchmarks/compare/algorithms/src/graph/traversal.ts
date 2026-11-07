import { Graph } from "./graph";

export function breadthFirst(graph: Graph, start: number): Map<number, number> {
	const distance = new Map<number, number>();
	const queue = [start];
	let head = 0;
	distance.set(start, 0);
	while (head < queue.size()) {
		const vertex = queue[head++];
		const current = distance.get(vertex)!;
		for (const edge of graph.neighbors(vertex)) {
			if (!distance.has(edge.to)) {
				distance.set(edge.to, current + 1);
				queue.push(edge.to);
			}
		}
	}
	return distance;
}

export function depthFirstOrder(graph: Graph, start: number): Array<number> {
	const visited = new Set<number>();
	const order = new Array<number>();
	const stack = [start];
	while (stack.size() > 0) {
		const vertex = stack.pop()!;
		if (visited.has(vertex)) continue;
		visited.add(vertex);
		order.push(vertex);
		const neighbors = graph.neighbors(vertex);
		for (let index = neighbors.size() - 1; index >= 0; index--) {
			if (!visited.has(neighbors[index].to)) stack.push(neighbors[index].to);
		}
	}
	return order;
}

export function topologicalSort(graph: Graph): Array<number> | undefined {
	const indegree = new Map<number, number>();
	for (const vertex of graph.vertices()) indegree.set(vertex, 0);
	for (const edge of graph.edges()) indegree.set(edge.to, indegree.get(edge.to)! + 1);
	const ready = graph.vertices().filter((vertex) => indegree.get(vertex) === 0);
	const order = new Array<number>();
	while (ready.size() > 0) {
		const vertex = ready.shift()!;
		order.push(vertex);
		for (const edge of graph.neighbors(vertex)) {
			const remaining = indegree.get(edge.to)! - 1;
			indegree.set(edge.to, remaining);
			if (remaining === 0) ready.push(edge.to);
		}
	}
	return order.size() === graph.vertexCount() ? order : undefined;
}

export function connectedComponents(graph: Graph): Array<Array<number>> {
	const seen = new Set<number>();
	const components = new Array<Array<number>>();
	for (const vertex of graph.vertices()) {
		if (seen.has(vertex)) continue;
		const component = depthFirstOrder(graph, vertex);
		for (const member of component) seen.add(member);
		components.push(component);
	}
	return components;
}

export function stronglyConnected(graph: Graph): Array<Array<number>> {
	const visited = new Set<number>();
	const finish = new Array<number>();
	const visit = (vertex: number): void => {
		visited.add(vertex);
		for (const edge of graph.neighbors(vertex)) if (!visited.has(edge.to)) visit(edge.to);
		finish.push(vertex);
	};
	for (const vertex of graph.vertices()) if (!visited.has(vertex)) visit(vertex);
	const transposed = graph.transpose();
	const assigned = new Set<number>();
	const components = new Array<Array<number>>();
	for (let index = finish.size() - 1; index >= 0; index--) {
		const root = finish[index];
		if (assigned.has(root)) continue;
		const component = new Array<number>();
		const stack = [root];
		assigned.add(root);
		while (stack.size() > 0) {
			const vertex = stack.pop()!;
			component.push(vertex);
			for (const edge of transposed.neighbors(vertex)) {
				if (!assigned.has(edge.to)) {
					assigned.add(edge.to);
					stack.push(edge.to);
				}
			}
		}
		components.push(component);
	}
	return components;
}

export function hasCycle(graph: Graph): boolean {
	return graph.directed ? topologicalSort(graph) === undefined : graph.getEdgeCount() >= graph.vertexCount();
}

//!native
interface Node<K, V> {
	readonly key: K;
	nextNode?: Node<K, V> | undefined;
	previousNode?: Node<K, V> | undefined;
	readonly value: V;
}

function createLruCache<K, V>(callback: (key: K) => V, capacity = 256, keyTransform?: (key: K) => K): (key: K) => V {
	const map = new Map<K, Node<K, V>>();
	let headNode: Node<K, V> | undefined;
	let tailNode: Node<K, V> | undefined;
	let size = 0;

	function detach(node?: Node<K, V>): void {
		if (node === undefined) {
			return;
		}
		if (node.previousNode !== undefined) {
			node.previousNode.nextNode = node.nextNode;
		} else {
			headNode = node.nextNode;
		}
		if (node.nextNode !== undefined) {
			node.nextNode.previousNode = node.previousNode;
		} else {
			tailNode = node.previousNode;
		}
		node.previousNode = undefined;
		node.nextNode = undefined;
	}

	function attachHead(node: Node<K, V>): void {
		node.previousNode = undefined;
		node.nextNode = headNode;
		if (headNode !== undefined) {
			headNode.previousNode = node;
		}
		headNode = node;
		if (tailNode === undefined) {
			tailNode = node;
		}
	}

	function evictIfNeeded(): void {
		while (size > capacity) {
			const evicted = tailNode;
			if (evicted === undefined) {
				return;
			}
			detach(evicted);
			map.delete(evicted.key);
			size -= 1;
		}
	}

	function memoized(key: K): V {
		const transformedKey = keyTransform !== undefined ? keyTransform(key) : key;
		const existing = map.get(transformedKey);
		if (existing !== undefined) {
			detach(existing);
			attachHead(existing);
			return existing.value;
		}
		const value = callback(transformedKey);
		const node: Node<K, V> = { key: transformedKey, value };
		map.set(transformedKey, node);
		attachHead(node);
		size += 1;
		evictIfNeeded();
		return value;
	}

	return memoized;
}

function roundKey(key: number, precision: number): number {
	const factor = 10 ** precision;
	return key - (key % (1 / factor));
}

namespace LruMemoizeUtilities {
	export function lruMemoize<K, V>(callback: (key: K) => V, capacity = 256): (key: K) => V {
		return createLruCache(callback, capacity);
	}

	export function lruMemoizeNumeric<V>(callback: (key: number) => V, capacity = 256, precision = 4): (key: number) => V {
		return createLruCache(callback, capacity, (key: number) => roundKey(key, precision));
	}
}

export = LruMemoizeUtilities;

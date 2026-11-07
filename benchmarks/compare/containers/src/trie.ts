class TrieNode {
	public readonly children = new Map<string, TrieNode>();
	public terminal = false;
	public passCount = 0;
	public payload?: number;
}

export class Trie {
	private readonly root = new TrieNode();
	private wordCount = 0;
	private static created = 0;

	public constructor() {
		Trie.created++;
	}

	public static createdCount(): number {
		return Trie.created;
	}

	public static fromWords(words: ReadonlyArray<string>): Trie {
		const trie = new Trie();
		words.forEach((word, index) => trie.insert(word, index));
		return trie;
	}

	public size(): number {
		return this.wordCount;
	}

	public insert(word: string, payload?: number): void {
		let node = this.root;
		node.passCount++;
		for (let index = 1; index <= word.size(); index++) {
			const character = word.sub(index, index);
			let child = node.children.get(character);
			if (!child) {
				child = new TrieNode();
				node.children.set(character, child);
			}
			child.passCount++;
			node = child;
		}
		if (!node.terminal) this.wordCount++;
		node.terminal = true;
		node.payload = payload;
	}

	private locate(prefix: string): TrieNode | undefined {
		let node: TrieNode | undefined = this.root;
		for (let index = 1; index <= prefix.size() && node; index++) node = node.children.get(prefix.sub(index, index));
		return node;
	}

	public has(word: string): boolean {
		return this.locate(word)?.terminal ?? false;
	}

	public payloadOf(word: string): number | undefined {
		const node = this.locate(word);
		return node?.terminal ? node.payload : undefined;
	}

	public countPrefix(prefix: string): number {
		return this.locate(prefix)?.passCount ?? 0;
	}

	public *withPrefix(prefix: string): Generator<string> {
		const start = this.locate(prefix);
		if (!start) return;
		const stack: Array<[TrieNode, string]> = [[start, prefix]];
		while (stack.size() > 0) {
			const [node, word] = stack.pop()!;
			if (node.terminal) yield word;
			const keys = new Array<string>();
			for (const [key] of node.children) keys.push(key);
			keys.sort((left, right) => left > right);
			for (const key of keys) stack.push([node.children.get(key)!, word + key]);
		}
	}

	public complete(prefix: string, limit: number): Array<string> {
		const result = new Array<string>();
		for (const word of this.withPrefix(prefix)) {
			if (result.size() >= limit) break;
			result.push(word);
		}
		return result;
	}

	public longestCommonPrefix(): string {
		let node = this.root;
		let prefix = "";
		while (node.children.size() === 1 && !node.terminal) {
			for (const [key, child] of node.children) {
				prefix += key;
				node = child;
			}
		}
		return prefix;
	}

	public remove(word: string): boolean {
		if (!this.has(word)) return false;
		let node = this.root;
		node.passCount--;
		for (let index = 1; index <= word.size(); index++) {
			const key = word.sub(index, index);
			const child = node.children.get(key)!;
			child.passCount--;
			if (child.passCount === 0) {
				node.children.delete(key);
				this.wordCount--;
				return true;
			}
			node = child;
		}
		node.terminal = false;
		this.wordCount--;
		return true;
	}
}

import { LinearCongruential, isSorted, numberAscending, range, byKey, stringAscending, thenBy } from "./sort/compare";
import { heapSort, insertionSort, mergeSort, quickSort, selectionSort, shellSort } from "./sort/comparison-sorts";
import { bucketSort, countingSort, quickSelect, radixSort } from "./sort/integer-sorts";
import { binarySearch, bisectRoot, lowerBound, ternaryMaximum, twoSum } from "./sort/search";
import { Graph, kruskal } from "./graph/graph";
import { breadthFirst, connectedComponents, stronglyConnected, topologicalSort } from "./graph/traversal";
import { bellmanFord, dijkstra, floydWarshall, reconstructPath } from "./graph/shortest-path";
import { solveMaze } from "./graph/astar";
import { editDistance, longestCommonSubsequence, longestIncreasingSubsequence, longestPalindrome, maximumSubarray } from "./dp/sequences";
import { coinChangeWays, fibonacci, knapsack, matrixChainOrder, minimumCoins, partitionCount, rodCutting } from "./dp/optimization";
import { boyerMooreHorspool, knuthMorrisPratt, rabinKarp, wildcardMatch, zFunction } from "./text/matching";
import { caesar, isAnagram, runLengthDecode, runLengthEncode, suffixArray, topWords, wrapText } from "./text/transforms";
import { bigFibonacci, BigNatural, factorial } from "./math/bignum";
import { Matrix, fibonacciByMatrix } from "./math/matrix";
import { chineseRemainder, collatzLength, eulerTotient, modularPower, pascalRow, sieve } from "./math/number-theory";

type Sorter = (input: ReadonlyArray<number>, comparator: (a: number, b: number) => number) => Array<number>;

const random = new LinearCongruential(42);
const data = random.shuffle(range(0, 500));

const sorters: Record<string, Sorter> = {
	insertion: insertionSort,
	selection: selectionSort,
	merge: mergeSort,
	quick: quickSort,
	heap: heapSort,
	shell: shellSort,
};

for (const [name, sorter] of pairs(sorters)) {
	const started = os.clock();
	const sorted = sorter(data, numberAscending);
	print(name, isSorted(sorted, numberAscending), os.clock() - started);
}
print("counting", isSorted(countingSort(data), numberAscending));
print("radix", isSorted(radixSort(data), numberAscending));
print("bucket", isSorted(bucketSort(data), numberAscending));
print("select", quickSelect(data, 250));

const sortedData = quickSort(data, numberAscending);
print("search", binarySearch(sortedData, 123, numberAscending), lowerBound(sortedData, 77, numberAscending), twoSum(sortedData, 600));
print("ternary", ternaryMaximum((x) => -(x - 3) * (x - 3), -10, 10));
print("root", bisectRoot((x) => x * x - 2, 0, 2));

interface Person {
	name: string;
	age: number;
}
const people: Array<Person> = [
	{ name: "ada", age: 36 },
	{ name: "bob", age: 25 },
	{ name: "cy", age: 36 },
];
const byAgeThenName = thenBy<Person>(
	byKey((person: Person) => person.age, numberAscending),
	byKey((person: Person) => person.name, stringAscending),
);
print(mergeSort(people, byAgeThenName).map((person) => person.name).join(","));

const graph = new Graph(true);
const edges: Array<[number, number, number]> = [
	[0, 1, 4],
	[0, 2, 1],
	[2, 1, 2],
	[1, 3, 1],
	[2, 3, 5],
	[3, 4, 3],
];
for (const [from, to, weight] of edges) graph.addEdge(from, to, weight);
const shortest = dijkstra(graph, 0);
print("dijkstra", shortest.distance.get(4), reconstructPath(shortest.previous, 0, 4).join("->"));
print("bellman", bellmanFord(graph, 0)?.get(4));
print("floyd", floydWarshall(graph).get(0)?.get(3));
print("topo", topologicalSort(graph)?.join(","));
print("bfs", breadthFirst(graph, 0).size());
print("scc", stronglyConnected(graph).size(), connectedComponents(graph).size());
print("mst", kruskal(graph).size());
const maze = solveMaze(40, 40, 7);
print("astar", maze.cost, maze.expanded, maze.path.size());

print("lcs", longestCommonSubsequence("AGGTAB", "GXTXAYB"));
print("edit", editDistance("kitten", "sitting"));
print("lis", longestIncreasingSubsequence([10, 9, 2, 5, 3, 7, 101, 18]).join(","));
print("kadane", ...maximumSubarray([-2, 1, -3, 4, -1, 2, 1, -5, 4]));
print("palindrome", longestPalindrome("forgeeksskeegfor"));
print(
	"knapsack",
	knapsack(
		[
			{ name: "a", weight: 1, value: 1 },
			{ name: "b", weight: 3, value: 4 },
			{ name: "c", weight: 4, value: 5 },
			{ name: "d", weight: 5, value: 7 },
		],
		7,
	)[0],
);
print("coins", coinChangeWays([1, 2, 5], 100), minimumCoins([1, 5, 10, 25], 63));
print("chain", matrixChainOrder([40, 20, 30, 10, 30]));
print("fib", fibonacci(60), fibonacciByMatrix(30), partitionCount(100), rodCutting([1, 5, 8, 9, 10, 17, 17, 20], 8));

const corpus = "the quick brown fox jumps over the lazy dog the fox";
print("kmp", knuthMorrisPratt(corpus, "the").join(","), rabinKarp(corpus, "fox").join(","), boyerMooreHorspool(corpus, "o").size());
print("z", zFunction("aabxaab").join(","), wildcardMatch("abcdef", "a*d?f"));
print("rle", runLengthDecode(runLengthEncode("aaabccddddd")));
print("caesar", caesar(caesar("Hello, World", 3), -3), isAnagram("Listen", "Silent"));
print("suffix", suffixArray("banana").join(","));
print("top", topWords(corpus, 3).map(([word, count]) => `${word}=${count}`).join(" "));
print("wrap", wrapText(corpus, 16).size());

print("big", factorial(30).toString(), bigFibonacci(100).toString(), BigNatural.fromNumber(2).power(100).digitSum());
const [quotient, remainder] = BigNatural.fromString("123456789012345678901234567890").divideSmall(97);
print("divide", quotient.toString(), remainder);
const system = Matrix.from([
	[2, 1, -1],
	[-3, -1, 2],
	[-2, 1, 2],
]);
print("matrix", system.determinant(), system.solve([8, -11, -3])?.join(","));
print("primes", sieve(100).size(), modularPower(2, 100, 1000000007), eulerTotient(360), chineseRemainder([2, 3, 2], [3, 5, 7]));
print("pascal", pascalRow(10).join(" "));
const collatz = new Map<number, number>();
let longest = 0;
for (let start = 1; start < 3000; start++) longest = math.max(longest, collatzLength(start, collatz));
print("collatz", longest);

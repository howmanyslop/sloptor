import { AbstractCollection } from "./collection";
import { LinkedList } from "./linked-list";
import { OverflowPolicy, RingBuffer, RollingAverage } from "./ring-buffer";
import { Deque, isPalindromeSequence, slidingWindowMaximum } from "./deque";
import { BinaryHeap, MedianTracker, TopK } from "./heap";
import { Trie } from "./trie";
import { LruCache, memoizeWithCache } from "./lru-cache";
import { BitSet, primeBits } from "./bitset";
import { IntervalTree, mergeIntervals } from "./interval-tree";
import { MinStack, balancedBrackets, evaluatePostfix, toPostfix, tokenize } from "./stack";
import { BiMap, CountingSet, MultiMap } from "./multimap";
import { Leaderboard, SortedList } from "./sorted-list";

const list = LinkedList.of(1, 2, 3, 4, 5, 6);
list.pushFront(0);
list.removeWhere((value) => value % 3 === 0);
list.reverse();
print(list.describe(), list.at(1), list.map((value) => value * 10).toArray().join(","));
for (const value of list.reversed()) print("reversed", value);

const ring = new RingBuffer<string>(3);
for (const word of ["a", "b", "c", "d", "e"]) ring.push(word);
print(ring.describe(), ring.droppedCount(), ring.newest());
const strict = new RingBuffer<number>(2, OverflowPolicy.Reject);
print(strict.push(1), strict.push(2), strict.push(3));
const rolling = new RollingAverage(4);
for (const sample of [10, 20, 30, 40, 50, 60]) rolling.sample(sample);
print(rolling.average(), rolling.minimum(), rolling.maximum());

const deque = new Deque<number>();
deque.addAll([1, 2, 3, 4, 5]);
deque.rotate(2);
print(deque.describe(), deque.at(0), slidingWindowMaximum([1, 3, -1, -3, 5, 3, 6, 7], 3).join(","));
print(isPalindromeSequence(["r", "a", "c", "e", "c", "a", "r"]));

const heap = BinaryHeap.minimum([9, 4, 7, 1, 8, 2]);
print(heap.drain().join(","));
const top = new TopK<number>(3, (left, right) => left - right);
for (const value of [5, 1, 9, 3, 7, 6]) top.offer(value);
print(top.result().join(","));
const median = new MedianTracker();
for (const value of [5, 15, 1, 3, 8, 7]) median.add(value);
print("median", median.median());

const trie = Trie.fromWords(["car", "card", "care", "cart", "cat", "dog", "dot"]);
print(trie.complete("car", 10).join(","), trie.countPrefix("ca"), trie.payloadOf("dog"), trie.has("ca"));
trie.remove("card");
print(trie.size(), Trie.createdCount());

const cache = new LruCache<string, number>(2, 10);
const disconnect = cache.onEvict((key, value) => print("evicted", key, value));
cache.set("a", 1);
cache.set("b", 2);
cache.get("a");
cache.set("c", 3);
cache.advance(20);
print(cache.get("a"), cache.hitRate(), cache.snapshot().evictions);
disconnect();
const slowSquare = memoizeWithCache<number, number>(16, (value) => value * value);
print(slowSquare(12), slowSquare(12));

const evens = BitSet.fromIndices(64, [0, 2, 4, 6, 8, 10]);
const small = BitSet.fromIndices(64, [0, 1, 2, 3, 4]);
print(evens.union(small).cardinality(), evens.intersection(small).toString().sub(1, 12), primeBits(100).cardinality());

const tree = new IntervalTree();
tree.insert(15, 20, "a");
tree.insert(10, 30, "b");
tree.insert(17, 19, "c");
tree.insert(5, 20, "d");
tree.insert(12, 15, "e");
tree.insert(30, 40, "f");
print(tree.stab(18).map((interval) => interval.label).join(","), tree.height());
const ordered = new Array<string>();
for (const interval of tree.inOrder()) ordered.push(interval.label);
print(ordered.join(""), mergeIntervals(tree.query(0, 100)).size());

const minStack = new MinStack();
for (const value of [5, 3, 7, 2, 8]) minStack.push(value);
minStack.pop();
minStack.pop();
print(minStack.minimum(), evaluatePostfix(toPostfix(tokenize("3 + 4 * 2 / (1 - 5) ^ 2"))), balancedBrackets("{[()()]}"));

const groups = MultiMap.groupBy(["apple", "avocado", "banana", "blueberry", "cherry"], (word) => word.sub(1, 1));
print(groups.get("b").join(","), groups.keyCount(), groups.invert().size());
const bimap = new BiMap<string, number>();
bimap.set("one", 1);
bimap.set("uno", 1);
print(bimap.getKey(1), bimap.getValue("one"), bimap.size());
const counter = new CountingSet<string>();
for (const word of "a b a c b a d".split(" ")) counter.add(word);
print(counter.mostCommon(2).map(([word, count]) => `${word}${count}`).join(" "));

const sorted = new SortedList<number>();
sorted.addAll([50, 10, 40, 20, 30]);
const window = new Array<number>();
for (const value of sorted.between(15, 45)) window.push(value);
print(window.join(","), sorted.floor(35), sorted.ceiling(35), sorted.contains(40));
const board = new Leaderboard();
board.submit("ana", 90);
board.submit("ben", 75);
board.submit("cat", 99);
board.submit("ben", 100);
print(board.top(2).join(" "), board.placement("ana"));
print("collections", AbstractCollection.liveCount());

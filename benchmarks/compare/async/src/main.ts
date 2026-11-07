import { AsyncCache, EventLog, SingleFlight } from "./cache";
import { Channel, Mutex, pipeline } from "./channel";
import { Latch, allSettled, firstSuccessful, mapConcurrent, pollUntil, raceLabeled, sequence } from "./combinators";
import { FakeService, FlakyError, UserRecord } from "./fake-service";
import { JobQueue, JobState } from "./job-queue";
import { KeyedLimiter, SlidingWindow, TokenBucket, limited } from "./rate-limiter";
import { Scheduler, cooperativeLoop, deferBatch, staggered } from "./scheduler";
import { Stopwatch, TimeoutError, debounce, retry, retryWithLibrary, sleep, throttle, withTimeout } from "./timing";

const log = new EventLog();
const service = new FakeService(7, 0.25, 0.05);

async function demoRetries(): Promise<void> {
	const user = await retry((attempt) => {
		log.record(`fetch attempt ${attempt}`);
		return service.fetchUser(3);
	}, 6);
	log.record(`fetched ${user.name}`);
	const viaLibrary = await retryWithLibrary(() => service.fetchUser(4), 6).catch(() => undefined);
	log.record(`library retry ${viaLibrary?.name ?? "failed"}`);
	try {
		await withTimeout(sleep(5), 0.1, "slow sleep");
	} catch (error) {
		if (error instanceof TimeoutError) log.record(error.describe());
	}
}

async function demoConcurrency(): Promise<void> {
	const ids = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
	const watch = new Stopwatch();
	const users = await mapConcurrent(ids, 3, (id) => retry(() => service.fetchUser(id), 5));
	log.record(`mapConcurrent ${users.size()} in ${string.format("%.2f", watch.lap())}s`);
	const settled = await allSettled(ids.map((id) => service.fetchUser(id)));
	const failures = settled.filter((entry) => entry.status === "rejected").size();
	log.record(`allSettled failures=${failures}`);
	const all = await Promise.all([service.topScores(3), sleep(0.01)]).catch(() => undefined);
	log.record(`top ${all ? all[0].map((user) => user.name).join(",") : "failed"}`);
	const [winner] = await raceLabeled([
		["fast", sleep(0.01)],
		["slow", sleep(0.2)],
	]);
	log.record(`race winner ${winner}`);
	const fallback = await firstSuccessful<UserRecord>([() => Promise.reject("down") as Promise<UserRecord>, () => service.fetchUser(99), () => retry(() => service.fetchUser(5), 5)]);
	log.record(`fallback ${fallback?.name}`);
	const ordered = await sequence([() => sleep(0.01).then(() => "a"), () => Promise.resolve("b")]);
	log.record(`sequence ${ordered.join("")}`);
}

async function demoQueue(): Promise<void> {
	const queue = new JobQueue(2);
	const ids = new Array<number>();
	for (let index = 0; index < 8; index++) {
		const [id] = queue.enqueue(
			async () => {
				await sleep(0.02 * (index % 3));
				if (index === 5) throw new FlakyError(index);
				return index * index;
			},
			{ priority: index % 2, label: `square-${index}` },
		);
		ids.push(id);
	}
	queue.cancel(ids[7]);
	await queue.onIdle();
	const stats = queue.snapshot();
	log.record(`queue done=${stats.completed} failed=${stats.failed} cancelled=${stats.cancelled} peak=${stats.peakRunning}`);
	log.record(`state of 6 is ${queue.stateOf(ids[5]) === JobState.Failed ? "failed" : "other"}`);
}

async function demoLimiters(): Promise<void> {
	const bucket = new TokenBucket(5, 20);
	let granted = 0;
	for (let index = 0; index < 10; index++) if (bucket.tryAcquire()) granted++;
	log.record(`bucket granted ${granted}`);
	const window = new SlidingWindow(3, 0.1);
	const save = limited(window, (id: number, score: number) => retry(() => service.saveScore(id, score), 5));
	const saved = await Promise.all([save(1, 10), save(2, 20), save(3, 30), save(4, 40), save(5, 50)]);
	log.record(`saved ${saved.size()} with window`);
	const perPlayer = new KeyedLimiter<string>(() => new TokenBucket(2, 1));
	const outcomes = ["ann", "ann", "ann", "bo"].map((name) => perPlayer.tryAcquire(name));
	log.record(`keyed ${outcomes.map((value) => (value ? "y" : "n")).join("")} keys=${perPlayer.size()}`);
}

async function demoChannels(): Promise<void> {
	const source = new Channel<number>(2);
	const sink = new Channel<string>(4);
	const worker = pipeline(source, sink, async (value) => {
		await sleep(0.005);
		return `v${value * 2}`;
	});
	for (let value = 1; value <= 6; value++) await source.send(value);
	source.close();
	const [processed, collected] = await Promise.all([worker, sink.drain()]);
	log.record(`pipeline ${processed} -> ${collected.join(",")}`);
	const mutex = new Mutex();
	let counter = 0;
	await Promise.all(
		[1, 2, 3, 4].map(() =>
			mutex.run(async () => {
				const snapshot = counter;
				await sleep(0.005);
				counter = snapshot + 1;
			}),
		),
	);
	log.record(`mutex counter ${counter}`);
}

async function demoScheduling(): Promise<void> {
	const scheduler = new Scheduler();
	let ticks = 0;
	const heartbeat = scheduler.every("heartbeat", 0.05, () => ticks++);
	scheduler.after("once", 0.12, () => log.record("one-shot fired"));
	scheduler.start();
	await sleep(0.3);
	heartbeat.cancel();
	scheduler.stop();
	log.record(`ticks ${ticks} pending ${scheduler.pendingCount()}`);
	const latch = new Latch(3);
	staggered(3, 0.01, () => latch.countDown());
	await latch.wait();
	let deferred = 0;
	deferBatch([() => deferred++, () => deferred++]);
	await pollUntil(() => deferred === 2, 0.01, 1);
	let processed = 0;
	cooperativeLoop([1, 2, 3, 4, 5, 6, 7, 8, 9, 10], 3, () => processed++);
	await pollUntil(() => processed === 10, 0.01, 1);
	const saves = new Array<string>();
	const debounced = debounce(0.02, (value: string) => saves.push(value));
	debounced("a");
	debounced("b");
	const throttled = throttle(1, (value: string) => saves.push(value));
	throttled("x");
	throttled("y");
	await sleep(0.05);
	log.record(`deferred=${deferred} processed=${processed} saves=${saves.join("")}`);
}

async function demoCaches(): Promise<void> {
	const cache = new AsyncCache<number, UserRecord>((id) => retry(() => service.fetchUser(id), 5), 10);
	await Promise.all([cache.get(1), cache.get(1), cache.get(2)]);
	await cache.get(1);
	const flight = new SingleFlight<string, Array<UserRecord>>();
	const [a, b] = await Promise.all([
		flight.run("top", () => retry(() => service.topScores(5), 5)),
		flight.run("top", () => retry(() => service.topScores(5), 5)),
	]);
	log.record(`cache ${cache.ratio()} single-flight shared=${a === b}`);
}

async function main(): Promise<void> {
	const watch = new Stopwatch();
	await demoRetries();
	await demoConcurrency();
	await demoQueue();
	await demoLimiters();
	await demoChannels();
	await demoScheduling();
	await demoCaches();
	log.record(`total ${service.callCount()} calls in ${string.format("%.2f", watch.elapsed())}s`);
	print(log.dump());
}

main().catch((reason) => warn(`main failed: ${tostring(reason)}`));

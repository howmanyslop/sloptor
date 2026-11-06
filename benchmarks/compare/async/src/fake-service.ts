import { sleep } from "./timing";

export interface UserRecord {
	readonly id: number;
	readonly name: string;
	readonly score: number;
}

export class FlakyError {
	public constructor(public readonly attempt: number) {}
}

export class FakeService {
	private calls = 0;
	private readonly random: Random;
	private readonly store = new Map<number, UserRecord>();

	public constructor(
		seed: number,
		private readonly failureRate: number,
		private readonly latency: number,
	) {
		this.random = new Random(seed);
		for (let id = 1; id <= 50; id++) this.store.set(id, { id, name: `user${id}`, score: this.random.NextInteger(0, 1000) });
	}

	public callCount(): number {
		return this.calls;
	}

	private async roundTrip(): Promise<void> {
		this.calls++;
		await sleep(this.latency * this.random.NextNumber(0.5, 1.5));
		if (this.random.NextNumber() < this.failureRate) throw new FlakyError(this.calls);
	}

	public async fetchUser(id: number): Promise<UserRecord> {
		await this.roundTrip();
		const user = this.store.get(id);
		if (!user) throw `no user ${id}`;
		return user;
	}

	public async saveScore(id: number, score: number): Promise<UserRecord> {
		await this.roundTrip();
		const user = this.store.get(id);
		if (!user) throw `no user ${id}`;
		const updated = { ...user, score };
		this.store.set(id, updated);
		return updated;
	}

	public async topScores(limit: number): Promise<Array<UserRecord>> {
		await this.roundTrip();
		const users = new Array<UserRecord>();
		for (const [, user] of this.store) users.push(user);
		users.sort((left, right) => left.score > right.score);
		const result = new Array<UserRecord>();
		for (let index = 0; index < math.min(limit, users.size()); index++) result.push(users[index]);
		return result;
	}
}

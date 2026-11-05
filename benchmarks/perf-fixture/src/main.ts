import { clamp, fibonacci, gcd, polynomial } from "./arithmetic";
import { chunk, unique, zip } from "./collections";
import { NamedEntity } from "./entities";
import { Dispatcher } from "./events";
import { formatRow, padLeft, replicate } from "./format";
import { isOk } from "./types";
import { requireNonEmpty, requireRange } from "./validation";

export function run(): string {
	const values = [5, 3, 9, 3, 7, 1, 9];
	const distinct = unique(values);
	const groups = chunk(distinct, 2);
	const zipped = zip(distinct, [1, 2, 3]);

	const entity = new NamedEntity(1, "widget", replicate("W", 3));
	entity.addTag("benchmark");

	const dispatcher = new Dispatcher<number>();
	let seen = 0;
	dispatcher.subscribe((payload) => {
		seen += payload;
	});
	dispatcher.emit(fibonacci(8));

	const checked = requireRange(clamp(seen, 0, 100), 0, 100);
	const label = isOk(checked) ? requireNonEmpty(entity.describe()) : { ok: false, error: "bad" };
	const cells = [padLeft(`${gcd(48, 18)}`, 3, "0"), `${polynomial(2, [1, 2, 3])}`];
	return `${formatRow(cells, [4, 4])} groups=${groups.length} pairs=${zipped.length} label=${label.ok}`;
}

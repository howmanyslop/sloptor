import type { GridPoint } from "../types";

export class Grid3<T extends defined> {
	private readonly cells: Array<T>;

	public constructor(
		public readonly width: number,
		public readonly height: number,
		public readonly depth: number,
		private readonly fill: T,
	) {
		this.cells = table.create(width * height * depth, fill);
	}

	public inBounds([x, y, z]: GridPoint): boolean {
		return x >= 0 && y >= 0 && z >= 0 && x < this.width && y < this.height && z < this.depth;
	}

	public get(point: GridPoint): T | undefined {
		if (!this.inBounds(point)) {
			return undefined;
		}
		return this.cells[this.indexOf(point)];
	}

	public getOr(point: GridPoint, fallback: T): T {
		return this.get(point) ?? fallback;
	}

	public set(point: GridPoint, value: T): boolean {
		if (!this.inBounds(point)) {
			return false;
		}
		this.cells[this.indexOf(point)] = value;
		return true;
	}

	public fillBox(origin: GridPoint, size: GridPoint, value: T): number {
		let written = 0;
		for (const point of this.boxPoints(origin, size)) {
			if (this.set(point, value)) {
				written += 1;
			}
		}
		return written;
	}

	public isBoxClear(origin: GridPoint, size: GridPoint, predicate: (value: T) => boolean): boolean {
		for (const point of this.boxPoints(origin, size)) {
			const value = this.get(point);
			if (value === undefined || !predicate(value)) {
				return false;
			}
		}
		return true;
	}

	public *boxPoints([ox, oy, oz]: GridPoint, [sx, sy, sz]: GridPoint): Generator<GridPoint> {
		for (let x = ox; x < ox + sx; x++) {
			for (let y = oy; y < oy + sy; y++) {
				for (let z = oz; z < oz + sz; z++) {
					yield [x, y, z];
				}
			}
		}
	}

	public count(predicate: (value: T) => boolean): number {
		return this.cells.reduce((total, value) => (predicate(value) ? total + 1 : total), 0);
	}

	public reset(): void {
		for (let index = 0; index < this.cells.size(); index++) {
			this.cells[index] = this.fill;
		}
	}

	public clone(): Grid3<T> {
		const copy = new Grid3<T>(this.width, this.height, this.depth, this.fill);
		for (let index = 0; index < this.cells.size(); index++) {
			copy.cells[index] = this.cells[index];
		}
		return copy;
	}

	private indexOf([x, y, z]: GridPoint): number {
		return x + this.width * (y + this.height * z);
	}
}

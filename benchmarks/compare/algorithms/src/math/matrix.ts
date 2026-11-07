export class Matrix {
	private readonly data: Array<number>;

	public constructor(
		public readonly rows: number,
		public readonly columns: number,
		fill = 0,
	) {
		this.data = [];
		for (let index = 0; index < rows * columns; index++) this.data.push(fill);
	}

	public static identity(size: number): Matrix {
		const result = new Matrix(size, size);
		for (let index = 0; index < size; index++) result.set(index, index, 1);
		return result;
	}

	public static from(values: ReadonlyArray<ReadonlyArray<number>>): Matrix {
		const result = new Matrix(values.size(), values[0]?.size() ?? 0);
		values.forEach((row, r) => row.forEach((value, c) => result.set(r, c, value)));
		return result;
	}

	public get(row: number, column: number): number {
		return this.data[row * this.columns + column];
	}

	public set(row: number, column: number, value: number): void {
		this.data[row * this.columns + column] = value;
	}

	public map(transform: (value: number, row: number, column: number) => number): Matrix {
		const result = new Matrix(this.rows, this.columns);
		for (let r = 0; r < this.rows; r++) for (let c = 0; c < this.columns; c++) result.set(r, c, transform(this.get(r, c), r, c));
		return result;
	}

	public add(other: Matrix): Matrix {
		return this.map((value, r, c) => value + other.get(r, c));
	}

	public scale(factor: number): Matrix {
		return this.map((value) => value * factor);
	}

	public transpose(): Matrix {
		const result = new Matrix(this.columns, this.rows);
		for (let r = 0; r < this.rows; r++) for (let c = 0; c < this.columns; c++) result.set(c, r, this.get(r, c));
		return result;
	}

	public multiply(other: Matrix): Matrix {
		assert(this.columns === other.rows, "dimension mismatch");
		const result = new Matrix(this.rows, other.columns);
		for (let r = 0; r < this.rows; r++) {
			for (let k = 0; k < this.columns; k++) {
				const left = this.get(r, k);
				if (left === 0) continue;
				for (let c = 0; c < other.columns; c++) result.set(r, c, result.get(r, c) + left * other.get(k, c));
			}
		}
		return result;
	}

	public power(exponent: number): Matrix {
		let result = Matrix.identity(this.rows);
		let base: Matrix = this;
		let remaining = exponent;
		while (remaining > 0) {
			if (remaining % 2 === 1) result = result.multiply(base);
			base = base.multiply(base);
			remaining = math.floor(remaining / 2);
		}
		return result;
	}

	public determinant(): number {
		assert(this.rows === this.columns, "square matrix required");
		const work = this.map((value) => value);
		let determinant = 1;
		for (let column = 0; column < this.columns; column++) {
			let pivot = column;
			for (let r = column + 1; r < this.rows; r++) {
				if (math.abs(work.get(r, column)) > math.abs(work.get(pivot, column))) pivot = r;
			}
			if (math.abs(work.get(pivot, column)) < 1e-12) return 0;
			if (pivot !== column) {
				work.swapRows(pivot, column);
				determinant = -determinant;
			}
			const pivotValue = work.get(column, column);
			determinant *= pivotValue;
			for (let r = column + 1; r < this.rows; r++) {
				const factor = work.get(r, column) / pivotValue;
				for (let c = column; c < this.columns; c++) work.set(r, c, work.get(r, c) - factor * work.get(column, c));
			}
		}
		return determinant;
	}

	public swapRows(first: number, second: number): void {
		for (let c = 0; c < this.columns; c++) {
			const temporary = this.get(first, c);
			this.set(first, c, this.get(second, c));
			this.set(second, c, temporary);
		}
	}

	public solve(vector: ReadonlyArray<number>): Array<number> | undefined {
		const size = this.rows;
		const augmented = new Matrix(size, size + 1);
		for (let r = 0; r < size; r++) {
			for (let c = 0; c < size; c++) augmented.set(r, c, this.get(r, c));
			augmented.set(r, size, vector[r]);
		}
		for (let column = 0; column < size; column++) {
			let pivot = column;
			for (let r = column + 1; r < size; r++) if (math.abs(augmented.get(r, column)) > math.abs(augmented.get(pivot, column))) pivot = r;
			if (math.abs(augmented.get(pivot, column)) < 1e-12) return undefined;
			augmented.swapRows(pivot, column);
			for (let r = 0; r < size; r++) {
				if (r === column) continue;
				const factor = augmented.get(r, column) / augmented.get(column, column);
				for (let c = column; c <= size; c++) augmented.set(r, c, augmented.get(r, c) - factor * augmented.get(column, c));
			}
		}
		const result = new Array<number>();
		for (let r = 0; r < size; r++) result.push(augmented.get(r, size) / augmented.get(r, r));
		return result;
	}

	public toString(): string {
		const lines = new Array<string>();
		for (let r = 0; r < this.rows; r++) {
			const cells = new Array<string>();
			for (let c = 0; c < this.columns; c++) cells.push(string.format("%8.3f", this.get(r, c)));
			lines.push(cells.join(" "));
		}
		return lines.join("\n");
	}
}

export function fibonacciByMatrix(n: number): number {
	return Matrix.from([
		[1, 1],
		[1, 0],
	])
		.power(n)
		.get(0, 1);
}

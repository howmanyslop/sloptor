export type Result<T, E = string> =
	| { readonly success: true; readonly value: T }
	| { readonly success: false; readonly error: E };

export function ok<T>(value: T): Result<T, never> {
	return { success: true, value };
}

export function err<E>(failure: E): Result<never, E> {
	return { success: false, error: failure };
}

export function mapResult<T, U, E>(result: Result<T, E>, mapper: (value: T) => U): Result<U, E> {
	return result.success ? ok(mapper(result.value)) : result;
}

export function unwrapOr<T, E>(result: Result<T, E>, fallback: T): T {
	return result.success ? result.value : fallback;
}

export function tryCall<T>(callback: () => T): Result<T, string> {
	const [success, value] = pcall(callback);
	if (success) {
		return ok(value as T);
	}
	return err(tostring(value));
}

export function retry<T>(attempts: number, delaySeconds: number, callback: () => T): Result<T, string> {
	let lastError = "no attempts made";
	for (let attempt = 1; attempt <= attempts; attempt++) {
		const result = tryCall(callback);
		if (result.success) {
			return result;
		}
		lastError = `attempt ${attempt}/${attempts}: ${result.error}`;
		if (attempt < attempts) {
			task.wait(delaySeconds * attempt);
		}
	}
	return err(lastError);
}

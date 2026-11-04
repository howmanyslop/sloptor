namespace HasEnum {
	export enum Inner2 {
		X,
	}
	export const c = Inner2.X;
}
export function useHasEnum() {
	print(HasEnum.c);
}

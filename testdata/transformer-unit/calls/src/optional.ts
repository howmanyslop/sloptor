interface Holder {
	value?: { num: number };
}
declare const holder: Holder;
const n = holder.value?.num;
print(n);

declare const cond: boolean;
declare const items: Array<ReactElement>;

const d = (
	<frame>
		hello world
		{cond && <frame />}
		{cond ? <frame /> : <textlabel />}
		{items}
	</frame>
);
const i = (
	<frame>
		one &amp; two&nbsp;three
		line2
	</frame>
);
const e = <frame>back\slash</frame>;
print(d, i, e);

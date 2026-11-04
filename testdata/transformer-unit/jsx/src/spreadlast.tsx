declare const arr: Array<ReactElement>;

const m = (
	<frame>
		{...arr}
		<frame />
	</frame>
);
print(m);

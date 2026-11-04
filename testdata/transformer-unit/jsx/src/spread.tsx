declare const extra: { Visible: boolean };
declare const maybe: { Visible: boolean } | undefined;

const e = <frame BackgroundTransparency={1} {...extra} Visible={false} />;
const e2 = <frame {...extra} Visible={false} />;
const a = <frame {...maybe} />;
const b = <frame BackgroundTransparency={1} {...maybe} />;
print(e, e2, a, b);

declare const flags: Array<boolean>;

const c = <frame Visible={flags.pop()!} Active={flags.pop()!} />;
print(c);

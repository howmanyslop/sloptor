//! `labels_test.go`: labeled `break`/`continue` lowering, end to end.

use super::{expect_compiled, keys, large_stack, output, testdata};
use crate::project::compile_project;

#[test]
fn labels_model() {
	let files = expect_compiled(large_stack(|| compile_project(&testdata("labels_model"))));
	let want = concat!(
		"-- Compiled with sloptor v3.0.0-beta.4\n",
		"local n = 0\n",
		"local _outer\n",
		"for i = 0, 2 do\n",
		"\t_outer = \"none\"\n",
		"\tfor j = 0, 2 do\n",
		"\t\tif j == 1 then\n",
		"\t\t\t_outer = \"continue\"\n",
		"\t\t\tbreak\n",
		"\t\tend\n",
		"\t\tn += 1\n",
		"\tend\n",
		"\tif _outer == \"continue\" then\n",
		"\t\tcontinue\n",
		"\tend\n",
		"\tn += 100\n",
		"end\n",
		"local _search\n",
		"for i = 0, 2 do\n",
		"\trepeat\n",
		"\t\tif i == 2 then\n",
		"\t\t\t_search = \"break\"\n",
		"\t\t\tbreak\n",
		"\t\tend\n",
		"\tuntil true\n",
		"\tif _search == \"break\" then\n",
		"\t\tbreak\n",
		"\tend\n",
		"\tn += i\n",
		"end\n",
		"repeat\n",
		"\tn += 1\n",
		"\tif n > 0 then\n",
		"\t\tbreak\n",
		"\tend\n",
		"\tn += 100\n",
		"until true\n",
		"print(n)\n",
		"return nil\n",
	);
	assert_eq!(output(&files, "out/main.luau"), want);
	assert_eq!(files.len(), 1, "produced files ({:?})", keys(&files));
}

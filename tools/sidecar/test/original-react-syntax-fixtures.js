// Small authored inputs; no F checkout or React package is copied or rewritten.
// Counter follows the real factory's existing src/index.test.ts positive case.
const counter = [
  "export function Counter(props: { count: number }) {",
  '  const label = "count: " + props.count;',
  "  return <div>{label}</div>;",
  "}",
  "",
].join("\n");

const sources = {
  "positive.tsx": counter,
  "unchanged.ts": "export const value = 1;\n",
  "unchanged-tsx.tsx": "export const value = 2;\n",
  "excluded.test.tsx": counter,
  "use-sync-external-store-with-selector.tsx": counter,
  "directive.ts": '"use memo";\nexport const directive = 1;\n',
  "comment.ts": '// "use memo" is only a comment.\nexport const comment = 1;\n',
  "crlf.tsx": ("// non-BMP: 🧪\n" + counter).replace(/\n/g, "\r\n"),
  "collision.tsx": [
    "const memoCache = 1, memoCache1 = 2, useMemoCache = 3, c = 4;",
    "export function Collision(props: { count: number }) {",
    '  const label = "🧪 " + props.count + memoCache + memoCache1 + useMemoCache + c;',
    "  return <div>{label}</div>;",
    "}",
    "",
  ].join("\n"),
  "realBabelError.tsx": "export function Broken(props: { count: number }) { return <div>{props.count}</span>; }\n",
  "remove.ts": "export const removed = true;\n",
  "declaration.d.ts": "export declare const declared: number;\n",
  "importer.ts": 'export { imported } from "../imported";\n',
};

module.exports = { counter, sources };

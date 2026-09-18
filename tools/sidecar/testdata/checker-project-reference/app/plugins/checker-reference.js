const ts = require("typescript");

module.exports = function checkerReference(checker) {
  return (context) => (sourceFile) => {
    const probe = sourceFile.statements
      .filter(ts.isVariableStatement)
      .flatMap((statement) => statement.declarationList.declarations)
      .find((declaration) => ts.isIdentifier(declaration.name) && declaration.name.text === "probe");
    const origin = probe?.initializer ? checker.typeToString(checker.getTypeAtLocation(probe.initializer)) : "missing";

    function visit(node) {
      if (ts.isStringLiteral(node) && node.text === "CHECKER_REFERENCE_VALUE") {
        return context.factory.createStringLiteral(origin);
      }
      return ts.visitEachChild(node, visit, context);
    }
    return ts.visitNode(sourceFile, visit);
  };
};

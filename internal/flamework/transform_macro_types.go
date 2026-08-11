package flamework

import (
	"errors"
	"fmt"
	"regexp"
	"strconv"
	"strings"

	"rotor/tsgo/ast"
	"rotor/tsgo/checker"
	"rotor/tsgo/nodebuilder"
	"rotor/tsgo/scanner"
)

var (
	ErrInvalidMacro       = errors.New("invalid Flamework macro")
	ErrMacroDiagnostic    = errors.New("flamework macro diagnostic")
	ErrGuardBuilderAbsent = errors.New("flamework guard builder is not configured")
)

type macroKind uint8

const (
	macroGeneric macroKind = iota
	macroCaller
	macroMany
	macroLiteral
	macroIntrinsic
)

type literalValue struct {
	text    string
	number  string
	boolean bool
	kind    ast.Kind
}

type userMacro struct {
	kind       macroKind
	metadata   string
	order      checker.TypeId
	target     *checker.Type
	items      []userMacro
	properties []macroProperty
	literal    literalValue
	intrinsic  string
	inputs     []*checker.Type
}

type macroProperty struct {
	name  string
	macro userMacro
}

// GuardBuildResult is the lane-C integration boundary. Statements must be
// emitted immediately before the statement containing Expression.
type GuardBuildResult struct {
	Expression *ast.Node
	Statements []*ast.Node
}

type GuardBuilder func(state *TransformState, trace *ast.Node, target *checker.Type) (GuardBuildResult, error)

// MacroRuntime makes the two upstream random inputs explicit and testable.
// RandomIndex returns an index in [0, upperBound).
type MacroRuntime struct {
	UUID        func() (string, error)
	RandomIndex func(upperBound int) (int, error)
	BuildGuard  GuardBuilder
}

// MacroTransformResult carries both the replacement expression and any guard
// prerequisites which the file visitor must insert before its statement.
type MacroTransformResult struct {
	Expression    *ast.Node
	Prerequisites []*ast.Node
	Imports       []MacroImport
}

type MacroImport struct {
	Module string
	Export string
	Local  string
}

type MacroError struct {
	Node               *ast.Node
	Message            string
	Cause              error
	RelatedInformation []MacroRelatedInformation
}

type MacroRelatedInformation struct {
	Node    *ast.Node
	Message string
}

func (e *MacroError) Error() string {
	return e.Message
}

func (e *MacroError) Unwrap() error {
	if e.Cause == nil {
		return ErrMacroDiagnostic
	}
	return errors.Join(ErrMacroDiagnostic, e.Cause)
}

func invalidMacro(node *ast.Node, format string, args ...any) error {
	return &MacroError{Node: node, Message: fmt.Sprintf(format, args...), Cause: ErrInvalidMacro}
}

func invalidMacroWithRelated(node *ast.Node, message string, related ...MacroRelatedInformation) error {
	return &MacroError{Node: node, Message: message, Cause: ErrInvalidMacro, RelatedInformation: related}
}

var metadataLinkKey = regexp.MustCompile(`([A-Za-z0-9_-]+)\s*\}?$`)

type macroMetadata struct {
	flags   map[string]struct{}
	symbols map[string][]*ast.Symbol
}

func (m macroMetadata) requested(name string) bool {
	if _, excluded := m.flags["~"+name]; excluded {
		return false
	}
	_, exact := m.flags[name]
	_, wildcard := m.flags["*"]
	return exact || wildcard
}

func readMacroMetadata(state *TransformState, declaration *ast.Node) macroMetadata {
	metadata := macroMetadata{flags: make(map[string]struct{}), symbols: make(map[string][]*ast.Symbol)}
	if declaration == nil {
		return metadata
	}
	for _, jsdoc := range declaration.JSDoc(nil) {
		tags := jsdoc.AsJSDoc().Tags
		if tags == nil {
			continue
		}
		for _, tag := range tags.Nodes {
			if tag.TagName().Text() != "metadata" {
				continue
			}
			for _, comment := range tag.Comments() {
				if ast.IsJSDocLinkLike(comment) {
					readMetadataLink(state, metadata, comment)
					continue
				}
				readMetadataFlags(metadata, scanner.GetTextOfNode(comment))
			}
		}
	}
	return metadata
}

func readMetadataFlags(metadata macroMetadata, text string) {
	for _, field := range strings.Fields(text) {
		field = strings.Trim(field, "{}@*/,")
		if field != "" {
			metadata.flags[field] = struct{}{}
		}
	}
}

func readMetadataLink(state *TransformState, metadata macroMetadata, link *ast.Node) {
	match := metadataLinkKey.FindStringSubmatch(scanner.GetTextOfNode(link))
	if len(match) != 2 || link.Name() == nil {
		return
	}
	symbol := state.checker.GetSymbolAtLocation(link.Name())
	if symbol == nil {
		return
	}
	if symbol.Flags&ast.SymbolFlagsAlias != 0 {
		symbol = state.checker.GetAliasedSymbol(symbol)
	}
	metadata.symbols[match[1]] = append(metadata.symbols[match[1]], symbol)
}

func buildCallerMacro(state *TransformState, trace *ast.Node, macro userMacro, runtime MacroRuntime) (*ast.Node, error) {
	sourceFile := ast.GetSourceFileOfNode(trace)
	start := scanner.SkipTrivia(sourceFile.Text(), trace.Pos())
	line, character := scanner.GetECMALineAndUTF16CharacterOfPosition(sourceFile, start)
	switch macro.metadata {
	case "line":
		return state.factory.NewNumericLiteral(strconv.Itoa(line+1), ast.TokenFlagsNone), nil
	case "character":
		return state.factory.NewNumericLiteral(strconv.Itoa(int(character)+1), ast.TokenFlagsNone), nil
	case "width":
		return state.factory.NewNumericLiteral(strconv.Itoa(trace.End()-start), ast.TokenFlagsNone), nil
	case "text":
		return state.factory.NewStringLiteral(scanner.GetTextOfNode(trace), ast.TokenFlagsNone), nil
	case "uuid":
		if runtime.UUID == nil {
			return nil, invalidMacro(trace, "Flamework UUID runtime is not configured")
		}
		uuid, err := runtime.UUID()
		if err != nil {
			return nil, fmt.Errorf("generate Flamework caller UUID: %w", err)
		}
		return state.factory.NewStringLiteral(uuid, ast.TokenFlagsNone), nil
	default:
		return state.factory.NewIdentifier("undefined"), nil
	}
}

func literalExpression(factory *ast.NodeFactory, value literalValue) *ast.Node {
	switch value.kind {
	case ast.KindStringLiteral:
		return factory.NewStringLiteral(value.text, ast.TokenFlagsNone)
	case ast.KindNumericLiteral:
		if len(value.number) > 0 && value.number[0] == '-' {
			return factory.NewPrefixUnaryExpression(ast.KindMinusToken, factory.NewNumericLiteral(value.number[1:], ast.TokenFlagsNone))
		}
		return factory.NewNumericLiteral(value.number, ast.TokenFlagsNone)
	case ast.KindTrueKeyword:
		if value.boolean {
			return factory.NewToken(ast.KindTrueKeyword)
		}
		return factory.NewToken(ast.KindFalseKeyword)
	default:
		return factory.NewIdentifier("undefined")
	}
}

func asNever(factory *ast.NodeFactory, expression *ast.Node) *ast.Node {
	return factory.NewAsExpression(expression, factory.NewKeywordTypeNode(ast.KindNeverKeyword))
}

func isUndefinedExpression(node *ast.Node) bool {
	return ast.IsIdentifier(node) && node.Text() == "undefined"
}

func stringLiteralValue(target *checker.Type) string {
	value, _ := target.AsLiteralType().Value().(string)
	return value
}

func generateUniversalTypeNode(state *TransformState, location *ast.Node, target *checker.Type) (*ast.Node, []*ast.Node, bool) {
	flags := nodebuilder.FlagsUseFullyQualifiedType | nodebuilder.FlagsWriteClassExpressionAsTypeLiteral
	typeNode := state.checker.TypeToTypeNode(target, location, flags, nil)
	if typeNode == nil {
		return nil, nil, false
	}
	return typeNode, nil, true
}

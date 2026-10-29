package render

import (
	"strconv"
	"strings"

	"rotor/internal/luau"
)

// typeSlot is the syntactic position a type is rendered into. The slot alone
// decides parenthesization, so builders never inspect rendered text. The Luau
// rules it encodes: `|` and `&` never mix unparenthesized, `A & B?` is a parse
// error, a function type inside a union, intersection, or optional must be
// parenthesized (its return type extends as far as possible), and
// `() -> (A, B)?` is an optional function, never an optional pack.
type typeSlot uint8

const (
	typeSlotTop                typeSlot = iota // annotations, alias bodies, type arguments, fields, pack elements
	typeSlotUnionMember                        // `A | _`
	typeSlotIntersectionMember                 // `A & _`
	typeSlotOptionalInner                      // `_?`
	typeSlotFunctionReturn                     // `() -> _`
	typeSlotVariadicInner                      // `..._`
)

// typeNeedsParens is the parenthesization table. Atoms (references,
// singletons, tables, arrays, typeof, packs, raw text) never take parentheses.
//
//	child \ slot          Top  Union  Intersection  Optional  FunctionReturn  Variadic
//	Union / Intersection  no   yes    yes           yes       yes             yes
//	Optional              no   no     yes           yes       no              no
//	Function              no   yes    yes           yes       no              no
func typeNeedsParens(node luau.TypeNode, slot typeSlot) bool {
	switch node.(type) {
	case *luau.TypeUnion, *luau.TypeIntersection:
		return slot != typeSlotTop
	case *luau.TypeOptional:
		return slot == typeSlotIntersectionMember || slot == typeSlotOptionalInner
	case *luau.TypeFunction:
		return slot == typeSlotUnionMember || slot == typeSlotIntersectionMember || slot == typeSlotOptionalInner
	}
	return false
}

// renderType renders a type into slot, parenthesized when the slot requires.
// The parentheses belong to the node's range, like an expression's.
func (s *RenderState) renderType(node luau.TypeNode, slot typeSlot) {
	if node == nil {
		panic("render: nil type")
	}
	if !typeNeedsParens(node, slot) {
		s.render(node)
		return
	}
	if !s.positions {
		s.write("(")
		renderNode(s, node)
		s.write(")")
		return
	}
	parent := s.currentRange
	index := s.beginRange(node)
	s.currentRange = index
	s.write("(")
	renderNode(s, node)
	s.write(")")
	s.endRange(index)
	s.currentRange = parent
}

func (s *RenderState) typeList(types *luau.List[luau.TypeNode]) {
	for current := types.Head; current != nil; current = current.Next {
		if current != types.Head {
			s.write(", ")
		}
		s.renderType(current.Value, typeSlotTop)
	}
}

// isMultilineUnion reports whether node lays its members out one per line.
// renderTypeAlias owns the line break before such a body.
func (s *RenderState) isMultilineUnion(node luau.TypeNode) bool {
	union, ok := node.(*luau.TypeUnion)
	return ok && union.Multiline && !s.singleLine
}

// isTypePack is IsTypePack for a child that may still be nil; renderType
// reports the nil.
func isTypePack(node luau.TypeNode) bool {
	return node != nil && luau.IsTypePack(node)
}

func typeAccessPrefix(access luau.TypeAccess) string {
	switch access {
	case luau.TypeAccessRead:
		return "read "
	case luau.TypeAccessWrite:
		return "write "
	}
	return ""
}

// escapeTypeString escapes a string singleton or a quoted property name for a
// double-quoted Luau literal.
func escapeTypeString(value string) string {
	var out strings.Builder
	for index := 0; index < len(value); index++ {
		c := value[index]
		switch c {
		case '\\':
			out.WriteString(`\\`)
		case '"':
			out.WriteString(`\"`)
		case '\n':
			out.WriteString(`\n`)
		case '\r':
			out.WriteString(`\r`)
		case '\t':
			out.WriteString(`\t`)
		default:
			if c < 0x20 || c == 0x7f {
				out.WriteString(`\x`)
				out.WriteString(strings.ToUpper(strconv.FormatUint(uint64(c)|0x100, 16)[1:]))
			} else {
				out.WriteByte(c)
			}
		}
	}
	return out.String()
}

func typeName(what, name string) string {
	if !luau.IsValidIdentifier(name) {
		panic("Invalid " + what + ": \"" + name + "\"")
	}
	return name
}

func renderTypeReference(s *RenderState, node *luau.TypeReference) {
	if node.Module != "" {
		s.write(typeName("type module", node.Module))
		s.write(".")
	}
	if node.Name == "typeof" {
		panic("render: `typeof` is not a type name; use a TypeTypeof node")
	}
	if node.Name == "nil" {
		s.write(node.Name)
	} else {
		s.write(typeName("type name", node.Name))
	}
	if node.Arguments != nil && node.Arguments.IsNonEmpty() {
		s.write("<")
		s.typeList(node.Arguments)
		s.write(">")
	}
}

func renderTypeStringSingleton(s *RenderState, node *luau.TypeStringSingleton) {
	s.write("\"")
	s.write(escapeTypeString(node.Value))
	s.write("\"")
}

func renderTypeBooleanSingleton(s *RenderState, node *luau.TypeBooleanSingleton) {
	if node.Value {
		s.write("true")
		return
	}
	s.write("false")
}

func renderTypeOptional(s *RenderState, node *luau.TypeOptional) {
	if isTypePack(node.Inner) {
		panic("render: a type pack cannot be optional")
	}
	s.renderType(node.Inner, typeSlotOptionalInner)
	s.write("?")
}

func renderTypeUnion(s *RenderState, node *luau.TypeUnion) {
	if node.Members.IsEmpty() {
		panic("render: empty union type")
	}
	multiline := node.Multiline && !s.singleLine
	for current := node.Members.Head; current != nil; current = current.Next {
		if multiline {
			// The first member sits where the caller left the cursor
			// (renderTypeAlias breaks the line and indents); the rest
			// continue at the same indent with a leading bar.
			if current != node.Members.Head {
				s.newline()
				s.writeIndent()
			}
			s.write("| ")
		} else if current != node.Members.Head {
			s.write(" | ")
		}
		s.renderType(current.Value, typeSlotUnionMember)
	}
}

func renderTypeIntersection(s *RenderState, node *luau.TypeIntersection) {
	if node.Members.IsEmpty() {
		panic("render: empty intersection type")
	}
	for current := node.Members.Head; current != nil; current = current.Next {
		if current != node.Members.Head {
			s.write(" & ")
		}
		s.renderType(current.Value, typeSlotIntersectionMember)
	}
}

// renderTypeTable mirrors renderMap when Multiline: one field per line with
// trailing commas and `}` at the enclosing indent.
func renderTypeTable(s *RenderState, node *luau.TypeTable) {
	if node.Fields.IsEmpty() {
		s.write("{}")
		return
	}
	indexers := 0
	node.Fields.ForEach(func(field luau.TypeTableField) {
		if _, ok := field.(*luau.TypeTableIndexer); ok {
			indexers++
		}
	})
	if indexers > 1 {
		panic("render: a table type cannot have more than one indexer")
	}
	if node.Multiline && !s.singleLine {
		s.write("{")
		s.newline()
		s.pushIndent()
		for field := node.Fields.Head; field != nil; field = field.Next {
			s.writeIndent()
			s.render(field.Value)
			s.write(",")
			s.newline()
		}
		s.popIndent()
		s.writeIndent()
		s.write("}")
		return
	}
	s.write("{ ")
	for field := node.Fields.Head; field != nil; field = field.Next {
		if field != node.Fields.Head {
			s.write(", ")
		}
		s.render(field.Value)
	}
	s.write(" }")
}

func renderTypeTableProp(s *RenderState, node *luau.TypeTableProp) {
	s.write(typeAccessPrefix(node.Access))
	if luau.IsValidIdentifier(node.Name) {
		s.write(node.Name)
	} else {
		s.write("[\"")
		s.write(escapeTypeString(node.Name))
		s.write("\"]")
	}
	s.write(": ")
	s.renderType(node.Value, typeSlotTop)
}

func renderTypeTableIndexer(s *RenderState, node *luau.TypeTableIndexer) {
	s.write(typeAccessPrefix(node.Access))
	s.write("[")
	s.renderType(node.Key, typeSlotTop)
	s.write("]: ")
	s.renderType(node.Value, typeSlotTop)
}

func renderTypeArray(s *RenderState, node *luau.TypeArray) {
	s.write("{ ")
	s.write(typeAccessPrefix(node.Access))
	s.renderType(node.Element, typeSlotTop)
	s.write(" }")
}

func renderTypeFunction(s *RenderState, node *luau.TypeFunction) {
	s.typeGenerics(node.Generics, false)
	s.write("(")
	for index, parameter := range node.Parameters {
		if index > 0 {
			s.write(", ")
		}
		if parameter.Name != "" {
			s.write(typeName("parameter name", parameter.Name))
			s.write(": ")
		}
		s.renderType(parameter.Type, typeSlotTop)
	}
	if node.VarArgs != nil {
		switch node.VarArgs.(type) {
		case *luau.TypeVariadic, *luau.TypeGenericPack:
		default:
			panic("render: function type varargs must be `...T` or `T...`")
		}
		if len(node.Parameters) > 0 {
			s.write(", ")
		}
		s.render(node.VarArgs)
	}
	s.write(") -> ")
	if node.Return == nil {
		panic("render: function type without a return; use an empty pack for `()`")
	}
	s.renderType(node.Return, typeSlotFunctionReturn)
}

func renderTypeTypeof(s *RenderState, node *luau.TypeTypeof) {
	s.write("typeof(")
	s.render(node.Expression)
	s.write(")")
}

func renderTypePack(s *RenderState, node *luau.TypePack) {
	s.write("(")
	for current := node.Elements.Head; current != nil; current = current.Next {
		if current != node.Elements.Head {
			s.write(", ")
		}
		if _, nested := current.Value.(*luau.TypePack); nested {
			panic("render: a type pack cannot contain a type pack")
		}
		if isTypePack(current.Value) && current.Next != nil {
			panic("render: a variadic or generic pack must be the last pack element")
		}
		s.renderType(current.Value, typeSlotTop)
	}
	s.write(")")
}

func renderTypeVariadic(s *RenderState, node *luau.TypeVariadic) {
	if isTypePack(node.Inner) {
		panic("render: a variadic must wrap a type, not a pack")
	}
	s.write("...")
	s.renderType(node.Inner, typeSlotVariadicInner)
}

func renderTypeGenericPack(s *RenderState, node *luau.TypeGenericPack) {
	s.write(typeName("generic pack name", node.Name))
	s.write("...")
}

func renderTypeRaw(s *RenderState, node *luau.TypeRaw) {
	s.write(node.Text)
}

// typeGenerics writes `<K, V = number, U... = ...string>`, or nothing for an
// empty list. Type parameters precede packs, a parameter after a defaulted
// one needs a default too, and defaults are legal only on type aliases.
func (s *RenderState) typeGenerics(generics []luau.TypeGeneric, allowDefaults bool) {
	if len(generics) == 0 {
		return
	}
	s.write("<")
	seenPack, seenDefault := false, false
	for index, generic := range generics {
		if index > 0 {
			s.write(", ")
		}
		if generic.IsPack {
			seenPack = true
		} else if seenPack {
			panic("render: generic type parameters must come before generic packs")
		}
		s.write(typeName("generic name", generic.Name))
		if generic.IsPack {
			s.write("...")
		}
		if generic.Default == nil {
			if seenDefault {
				panic("render: a generic after a defaulted one needs a default")
			}
			continue
		}
		if !allowDefaults {
			panic("render: generic defaults are only allowed on type aliases")
		}
		if generic.IsPack != isTypePack(generic.Default) {
			panic("render: a generic pack default must be a pack, a type default a type")
		}
		seenDefault = true
		s.write(" = ")
		s.renderType(generic.Default, typeSlotTop)
	}
	s.write(">")
}

func renderTypeAlias(s *RenderState, node *luau.TypeAlias) {
	if node.Body == nil {
		panic("render: type alias " + node.Name + " has no body")
	}
	if isTypePack(node.Body) {
		panic("render: a type pack cannot be an alias body")
	}
	if node.Export {
		s.write("export ")
	}
	s.write("type ")
	s.write(typeName("type alias name", node.Name))
	s.typeGenerics(node.Generics, true)
	s.write(" =")
	if s.isMultilineUnion(node.Body) {
		s.newline()
		s.pushIndent()
		s.writeIndent()
		s.renderType(node.Body, typeSlotTop)
		s.popIndent()
	} else {
		s.write(" ")
		s.renderType(node.Body, typeSlotTop)
	}
	s.newline()
}

func renderTypeFunctionDeclaration(s *RenderState, node *luau.TypeFunctionDeclaration) {
	if node.Export {
		s.write("export ")
	}
	s.write("type function ")
	s.write(typeName("type function name", node.Name))
	s.write("(")
	s.parameters(node)
	s.write(")")
	s.newline()
	s.block(node.Statements)
	s.writeIndent()
	s.closing("end")
	s.newline()
}

func renderCastExpression(s *RenderState, node *luau.CastExpression) {
	parens := castNeedsParentheses(node)
	if parens {
		s.write("(")
	}
	operand := castOperandNeedsParentheses(node.Expression)
	if operand {
		s.write("(")
	}
	s.render(node.Expression)
	if operand {
		s.write(")")
	}
	s.write(" ")
	s.operator("::")
	s.write(" ")
	s.renderType(node.Type, typeSlotTop)
	if parens {
		s.write(")")
	}
}

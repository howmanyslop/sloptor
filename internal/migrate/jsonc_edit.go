package migrate

import (
	"bytes"
	"strings"
)

func rewriteTargetPlugins(target *jsoncDocument, plugins []pluginEntry, flamework pluginEntry, declaredInTarget bool) ([]byte, error) {
	compilerOptions := findProperty(target.root, "compilerOptions")
	if compilerOptions != nil && compilerOptions.value.kind != jsoncObject {
		return nil, &JSONCMigrationError{Path: target.path, Reason: "compilerOptions must be an object"}
	}
	newline := detectNewline(target.raw)
	indentUnit := detectIndentUnit(target.raw)
	if compilerOptions != nil {
		if property := findProperty(compilerOptions.value, "plugins"); property != nil {
			if declaredInTarget {
				return removeArrayElement(target.raw, property.value, flamework.node), nil
			}
			arrayIndent := lineIndent(target.raw, property.value.start)
			rendered := renderPlugins(plugins, target, arrayIndent, indentUnit, newline, property.value.trailingComma)
			return replaceBytes(target.raw, property.value.start, property.value.end, rendered), nil
		}
		objectIndent := lineIndent(target.raw, compilerOptions.value.start)
		return insertProperty(target.raw, compilerOptions.value, "plugins", renderPlugins(plugins, target, objectIndent+indentUnit, indentUnit, newline, objectUsesTrailingComma(compilerOptions.value)), objectIndent, indentUnit, newline), nil
	}
	rootIndent := lineIndent(target.raw, target.root.start)
	pluginsText := renderPlugins(plugins, target, rootIndent+indentUnit+indentUnit, indentUnit, newline, objectUsesTrailingComma(target.root))
	compilerText := []byte("{" + newline + rootIndent + indentUnit + indentUnit + `"plugins": ` + string(pluginsText) + newline + rootIndent + indentUnit + "}")
	return insertProperty(target.raw, target.root, "compilerOptions", compilerText, rootIndent, indentUnit, newline), nil
}

func removeArrayElement(raw []byte, array, element *jsoncNode) []byte {
	index := -1
	for candidate, current := range array.elements {
		if current == element {
			index = candidate
			break
		}
	}
	if index < 0 {
		return bytes.Clone(raw)
	}
	if len(array.elements) == 1 {
		return replaceBytes(raw, element.start, element.end, nil)
	}
	if index < len(array.elements)-1 {
		comma := array.commas[index]
		withoutComma := replaceBytes(raw, comma, comma+1, nil)
		return replaceBytes(withoutComma, element.start, element.end, nil)
	}
	if array.trailingComma {
		comma := array.commas[index]
		withoutComma := replaceBytes(raw, comma, comma+1, nil)
		return replaceBytes(withoutComma, element.start, element.end, nil)
	}
	comma := array.commas[index-1]
	withoutElement := replaceBytes(raw, element.start, element.end, nil)
	return replaceBytes(withoutElement, comma, comma+1, nil)
}

func renderPlugins(plugins []pluginEntry, target *jsoncDocument, propertyIndent, indentUnit, newline string, trailingComma bool) []byte {
	if len(plugins) == 0 {
		return []byte("[]")
	}
	itemIndent := propertyIndent + indentUnit
	var builder strings.Builder
	builder.WriteByte('[')
	builder.WriteString(newline)
	for index, plugin := range plugins {
		builder.WriteString(itemIndent)
		raw := bytes.TrimSpace(plugin.document.raw[plugin.node.start:plugin.node.end])
		text := strings.ReplaceAll(strings.ReplaceAll(string(raw), "\r\n", "\n"), "\n", newline+itemIndent)
		builder.WriteString(text)
		if index < len(plugins)-1 || trailingComma {
			builder.WriteByte(',')
		}
		builder.WriteString(newline)
	}
	builder.WriteString(propertyIndent)
	builder.WriteByte(']')
	return []byte(builder.String())
}

func insertProperty(raw []byte, object *jsoncNode, name string, value []byte, objectIndent, indentUnit, newline string) []byte {
	propertyIndent := objectIndent + indentUnit
	text := `"` + name + `": ` + string(value)
	insertion := ""
	if len(object.properties) == 0 {
		insertion = newline + propertyIndent + text + newline + objectIndent
	} else if object.trailingComma {
		insertion = newline + propertyIndent + text + "," + newline + objectIndent
	} else {
		insertion = "," + newline + propertyIndent + text + newline + objectIndent
	}
	return replaceBytes(raw, object.end-1, object.end-1, []byte(insertion))
}

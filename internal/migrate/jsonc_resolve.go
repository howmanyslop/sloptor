package migrate

import (
	"bytes"
	"fmt"
	"os"
	"path/filepath"
)

func resolveEffectivePlugins(path string, documents map[string]*jsoncDocument, active map[string]bool) (effectivePlugins, error) {
	cleanPath := filepath.Clean(path)
	if active[cleanPath] {
		return effectivePlugins{}, &JSONCMigrationError{Path: cleanPath, Reason: "extends cycle detected"}
	}
	active[cleanPath] = true
	defer delete(active, cleanPath)

	document, ok := documents[cleanPath]
	if !ok {
		parsed, err := readJSONCDocument(cleanPath)
		if err != nil {
			return effectivePlugins{}, err
		}
		document = parsed
		documents[cleanPath] = parsed
	}

	result := effectivePlugins{}
	extends, err := documentExtends(document)
	if err != nil {
		return effectivePlugins{}, err
	}
	for _, extended := range extends {
		resolved, err := resolveExtendsPath(cleanPath, extended)
		if err != nil {
			return effectivePlugins{}, &JSONCMigrationError{Path: cleanPath, Reason: err.Error()}
		}
		base, err := resolveEffectivePlugins(resolved, documents, active)
		if err != nil {
			return effectivePlugins{}, err
		}
		if base.declared {
			result = base
		}
	}
	local, err := documentPlugins(document)
	if err != nil {
		return effectivePlugins{}, err
	}
	if local.declared {
		result = local
	}
	return result, nil
}

func readJSONCDocument(path string) (*jsoncDocument, error) {
	raw, err := os.ReadFile(path)
	if err != nil {
		return nil, &JSONCMigrationError{Path: path, Reason: fmt.Sprintf("read missing or inaccessible config: %v", err)}
	}
	parser := jsoncParser{path: path, src: raw}
	if bytes.HasPrefix(raw, []byte{0xef, 0xbb, 0xbf}) {
		parser.pos = 3
	}
	root, err := parser.parseValue()
	if err != nil {
		return nil, err
	}
	if err := parser.skipTrivia(); err != nil {
		return nil, err
	}
	if parser.pos != len(raw) || root.kind != jsoncObject {
		return nil, parser.failure("root must be one JSON object")
	}
	return &jsoncDocument{path: path, raw: raw, root: root}, nil
}

func documentExtends(document *jsoncDocument) ([]string, error) {
	property := findProperty(document.root, "extends")
	if property == nil {
		return nil, nil
	}
	if property.value.kind == jsoncString {
		value, err := decodeString(document, property.value)
		if err != nil {
			return nil, err
		}
		return []string{value}, nil
	}
	if property.value.kind != jsoncArray {
		return nil, &JSONCMigrationError{Path: document.path, Reason: "extends must be a string or array of strings"}
	}
	values := make([]string, 0, len(property.value.elements))
	for _, element := range property.value.elements {
		if element.kind != jsoncString {
			return nil, &JSONCMigrationError{Path: document.path, Reason: "extends array must contain only strings"}
		}
		value, err := decodeString(document, element)
		if err != nil {
			return nil, err
		}
		values = append(values, value)
	}
	return values, nil
}

func documentPlugins(document *jsoncDocument) (effectivePlugins, error) {
	compilerOptions := findProperty(document.root, "compilerOptions")
	if compilerOptions == nil {
		return effectivePlugins{}, nil
	}
	if compilerOptions.value.kind != jsoncObject {
		return effectivePlugins{}, &JSONCMigrationError{Path: document.path, Reason: "compilerOptions must be an object"}
	}
	property := findProperty(compilerOptions.value, "plugins")
	if property == nil {
		return effectivePlugins{}, nil
	}
	if property.value.kind != jsoncArray {
		return effectivePlugins{}, &JSONCMigrationError{Path: document.path, Reason: "compilerOptions.plugins must be an array"}
	}
	plugins := make([]pluginEntry, 0, len(property.value.elements))
	for _, element := range property.value.elements {
		if element.kind != jsoncObject {
			return effectivePlugins{}, &JSONCMigrationError{Path: document.path, Reason: "each compilerOptions.plugins entry must be an object"}
		}
		transformProperty := findProperty(element, "transform")
		if transformProperty == nil {
			continue
		}
		if transformProperty.value.kind != jsoncString {
			return effectivePlugins{}, &JSONCMigrationError{Path: document.path, Reason: "each compilerOptions.plugins entry must have a string transform"}
		}
		transform, err := decodeString(document, transformProperty.value)
		if err != nil {
			return effectivePlugins{}, err
		}
		plugins = append(plugins, pluginEntry{transform: transform, node: element, document: document})
	}
	return effectivePlugins{document: document, array: property.value, plugins: plugins, declared: true}, nil
}

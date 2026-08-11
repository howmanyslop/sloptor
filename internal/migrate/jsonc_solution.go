package migrate

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
)

func PlanFlameworkTSConfigTree(path string) ([]TSConfigChange, error) {
	absPath, err := filepath.Abs(path)
	if err != nil {
		return nil, &JSONCMigrationError{Path: path, Reason: fmt.Sprintf("resolve solution config: %v", err)}
	}
	paths, err := solutionTSConfigPaths(absPath)
	if err != nil {
		return nil, err
	}
	changes := make([]TSConfigChange, 0, len(paths))
	for _, configPath := range paths {
		change, planErr := PlanFlameworkTSConfig(configPath)
		if errors.Is(planErr, ErrNoFlameworkPlugin) {
			continue
		}
		if planErr != nil {
			return nil, planErr
		}
		changes = append(changes, change)
	}
	if len(changes) == 0 {
		return nil, flameworkCountError(absPath, 0)
	}
	return changes, nil
}

func solutionTSConfigPaths(root string) ([]string, error) {
	paths := make([]string, 0, 1)
	seen := make(map[string]struct{})
	var visit func(string) error
	visit = func(path string) error {
		path, err := filepath.Abs(filepath.Clean(path))
		if err != nil {
			return &JSONCMigrationError{Path: path, Reason: fmt.Sprintf("resolve referenced config: %v", err)}
		}
		if _, ok := seen[path]; ok {
			return nil
		}
		seen[path] = struct{}{}
		document, err := readJSONCDocument(path)
		if err != nil {
			return err
		}
		paths = append(paths, path)
		references, err := documentReferences(document)
		if err != nil {
			return err
		}
		for _, reference := range references {
			resolved, resolveErr := resolveProjectReference(path, reference)
			if resolveErr != nil {
				return resolveErr
			}
			if err := visit(resolved); err != nil {
				return err
			}
		}
		return nil
	}
	if err := visit(root); err != nil {
		return nil, err
	}
	return paths, nil
}

func documentReferences(document *jsoncDocument) ([]string, error) {
	property := findProperty(document.root, "references")
	if property == nil {
		return nil, nil
	}
	if property.value.kind != jsoncArray {
		return nil, &JSONCMigrationError{Path: document.path, Reason: "references must be an array"}
	}
	references := make([]string, 0, len(property.value.elements))
	for _, element := range property.value.elements {
		if element.kind != jsoncObject {
			return nil, &JSONCMigrationError{Path: document.path, Reason: "each references entry must be an object"}
		}
		pathProperty := findProperty(element, "path")
		if pathProperty == nil || pathProperty.value.kind != jsoncString {
			return nil, &JSONCMigrationError{Path: document.path, Reason: "each references entry must have a string path"}
		}
		reference, err := decodeString(document, pathProperty.value)
		if err != nil {
			return nil, err
		}
		references = append(references, reference)
	}
	return references, nil
}

func resolveProjectReference(from, reference string) (string, error) {
	base := filepath.Join(filepath.Dir(from), filepath.FromSlash(reference))
	candidates := []string{base}
	if filepath.Ext(base) == "" {
		candidates = append(candidates, base+".json")
	}
	candidates = append(candidates, filepath.Join(base, "tsconfig.json"))
	for _, candidate := range candidates {
		info, err := os.Stat(candidate)
		if errors.Is(err, os.ErrNotExist) {
			continue
		}
		if err != nil {
			return "", &JSONCMigrationError{Path: from, Reason: fmt.Sprintf("resolve project reference %q: %v", reference, err)}
		}
		if info.IsDir() {
			continue
		}
		return filepath.Abs(candidate)
	}
	return "", &JSONCMigrationError{Path: from, Reason: fmt.Sprintf("project reference %q does not resolve to a tsconfig file", reference)}
}

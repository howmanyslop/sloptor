package compile

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
)

func readProjectReferencePaths(tsConfigPath string) ([]string, bool, []string, error) {
	config, err := readProjectConfig(tsConfigPath)
	if err != nil {
		return nil, false, nil, err
	}
	references, _ := config["references"].([]any)
	paths := make([]string, 0, len(references))
	for _, reference := range references {
		entry, ok := reference.(map[string]any)
		if !ok {
			continue
		}
		path, ok := entry["path"].(string)
		if !ok {
			continue
		}
		if !filepath.IsAbs(path) {
			path = filepath.Join(filepath.Dir(tsConfigPath), filepath.FromSlash(path))
		}
		if filepath.Ext(path) != ".json" {
			path = filepath.Join(path, "tsconfig.json")
		}
		paths = append(paths, filepath.Clean(path))
	}
	fileSpecs, err := readEffectiveProjectFileSpecs(tsConfigPath, make(map[string]struct{}))
	if err != nil {
		return nil, false, nil, err
	}
	emptyFiles := fileSpecs.files.present && fileSpecs.files.valid && len(fileSpecs.files.values) == 0
	emptyInclude := !fileSpecs.include.present || fileSpecs.include.valid && len(fileSpecs.include.values) == 0
	return paths, len(paths) > 0 && emptyFiles && emptyInclude, fileSpecs.configPaths, nil
}

type projectFileSpec struct {
	values  []any
	present bool
	valid   bool
}

type projectFileSpecs struct {
	files       projectFileSpec
	include     projectFileSpec
	configPaths []string
}

// readEffectiveProjectFileSpecs follows TypeScript's config inheritance for
// root file specs. files and include inherit independently; with multiple
// bases, a later base overrides only the properties it declares.
func readEffectiveProjectFileSpecs(tsConfigPath string, visiting map[string]struct{}) (projectFileSpecs, error) {
	normalized, err := filepath.Abs(tsConfigPath)
	if err != nil {
		return projectFileSpecs{}, err
	}
	normalized = filepath.Clean(normalized)
	if _, ok := visiting[normalized]; ok {
		return projectFileSpecs{}, fmt.Errorf("tsconfig extends cycle at %s", normalized)
	}
	visiting[normalized] = struct{}{}
	defer delete(visiting, normalized)

	config, err := readProjectConfig(normalized)
	if err != nil {
		return projectFileSpecs{}, err
	}

	result := projectFileSpecs{configPaths: []string{normalized}}
	extends, err := projectExtends(config, normalized)
	if err != nil {
		return projectFileSpecs{}, err
	}
	for _, extends := range extends {
		parent, err := resolveExtendedConfig(normalized, extends)
		if err != nil {
			return projectFileSpecs{}, err
		}
		parentSpecs, err := readEffectiveProjectFileSpecs(parent, visiting)
		if err != nil {
			return projectFileSpecs{}, err
		}
		if parentSpecs.files.present {
			result.files = parentSpecs.files
		}
		if parentSpecs.include.present {
			result.include = parentSpecs.include
		}
		result.configPaths = append(result.configPaths, parentSpecs.configPaths...)
	}
	if value, present := config["files"]; present {
		result.files = projectFileSpecFrom(value)
	}
	if value, present := config["include"]; present {
		result.include = projectFileSpecFrom(value)
	}
	return result, nil
}

func readProjectConfig(tsConfigPath string) (map[string]any, error) {
	data, err := os.ReadFile(tsConfigPath)
	if err != nil {
		return nil, err
	}
	var config map[string]any
	if err := json.Unmarshal([]byte(stripJSONC(string(data))), &config); err != nil {
		return nil, fmt.Errorf("Failed to parse tsconfig at %s: %w", tsConfigPath, err)
	}
	return config, nil
}

func projectExtends(config map[string]any, tsConfigPath string) ([]string, error) {
	raw, present := config["extends"]
	if !present {
		return nil, nil
	}
	switch value := raw.(type) {
	case string:
		return []string{value}, nil
	case []any:
		result := make([]string, 0, len(value))
		for _, entry := range value {
			path, ok := entry.(string)
			if !ok {
				return nil, fmt.Errorf("tsconfig extends at %s must contain only strings", tsConfigPath)
			}
			result = append(result, path)
		}
		return result, nil
	default:
		return nil, fmt.Errorf("tsconfig extends at %s must be a string or array of strings", tsConfigPath)
	}
}

func projectFileSpecFrom(value any) projectFileSpec {
	values, valid := value.([]any)
	return projectFileSpec{values: values, present: true, valid: valid}
}

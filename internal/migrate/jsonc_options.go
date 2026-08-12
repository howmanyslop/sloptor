package migrate

import (
	"fmt"
	"strconv"
)

func parseFlameworkOptions(plugin pluginEntry) (FlameworkOptions, error) {
	result := FlameworkOptions{}
	for _, property := range plugin.node.properties {
		switch property.key {
		case "transform":
		case "after", "afterDeclarations":
			value, err := decodeBool(plugin.document, property.value)
			if err != nil {
				return result, err
			}
			if value {
				return result, &JSONCMigrationError{Path: plugin.document.path, Reason: fmt.Sprintf("cannot represent Flamework transformer phase option %q", property.key)}
			}
		case "type":
			value, err := decodeString(plugin.document, property.value)
			if err != nil || value != "program" {
				return result, &JSONCMigrationError{Path: plugin.document.path, Reason: "cannot represent Flamework transformer type other than program"}
			}
		case "noSemanticDiagnostics":
			value, err := decodeBool(plugin.document, property.value)
			if err != nil {
				return result, err
			}
			result.NoSemanticDiagnostics = value
		case "salt":
			value, err := decodeString(plugin.document, property.value)
			if err != nil {
				return result, err
			}
			result.Salt = value
		case "hashPrefix":
			value, err := decodeString(plugin.document, property.value)
			if err != nil {
				return result, err
			}
			result.HashPrefix = value
		case "preloadIds":
			value, err := decodeBool(plugin.document, property.value)
			if err != nil {
				return result, err
			}
			result.PreloadIDs = value
		case "obfuscation":
			value, err := decodeBool(plugin.document, property.value)
			if err != nil {
				return result, err
			}
			result.Obfuscation = value
		case "idGenerationMode":
			value, err := decodeString(plugin.document, property.value)
			if err != nil {
				return result, err
			}
			switch value {
			case "full", "short", "tiny", "obfuscated":
				result.IDGenerationMode = value
			default:
				return result, &JSONCMigrationError{Path: plugin.document.path, Reason: fmt.Sprintf("unknown idGenerationMode %q", value)}
			}
		case "optimizations":
			limit, err := parseOptimizations(plugin.document, property.value)
			if err != nil {
				return result, err
			}
			result.Optimizations.GuardGenerationDedupLimit = limit
		default:
			return result, &JSONCMigrationError{Path: plugin.document.path, Reason: fmt.Sprintf("unknown or future Flamework option %q", property.key)}
		}
	}
	return result, nil
}

func parseOptimizations(document *jsoncDocument, node *jsoncNode) (*int, error) {
	if node.kind != jsoncObject {
		return nil, &JSONCMigrationError{Path: document.path, Reason: "optimizations must be an object"}
	}
	var limit *int
	for _, property := range node.properties {
		if property.key != "guardGenerationDedupLimit" {
			return nil, &JSONCMigrationError{Path: document.path, Reason: fmt.Sprintf("unknown or future Flamework optimization %q", property.key)}
		}
		if property.value.kind != jsoncNumber {
			return nil, &JSONCMigrationError{Path: document.path, Reason: "guardGenerationDedupLimit must be a nonnegative integer"}
		}
		value, err := strconv.Atoi(string(document.raw[property.value.start:property.value.end]))
		if err != nil || value < 0 {
			return nil, &JSONCMigrationError{Path: document.path, Reason: "guardGenerationDedupLimit must be a nonnegative integer"}
		}
		limit = &value
	}
	return limit, nil
}

func rejectDuplicateTransforms(path string, plugins []pluginEntry) error {
	seen := make(map[string]bool, len(plugins))
	for _, plugin := range plugins {
		if seen[plugin.transform] && plugin.transform != flameworkTransformer {
			return &JSONCMigrationError{Path: path, Reason: fmt.Sprintf("duplicate transformer %q makes after ordering unrepresentable", plugin.transform)}
		}
		seen[plugin.transform] = true
	}
	return nil
}

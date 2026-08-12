// allow: SIZE_OK — this assigned file contains the indivisible JSONC concrete-syntax parser and planner.
package migrate

import (
	"bytes"
	"errors"
	"fmt"
	"path/filepath"
)

const flameworkTransformer = "rbxts-transformer-flamework"

type TSConfigChangeStatus string

const TSConfigChangePlanned TSConfigChangeStatus = "planned"

type TSConfigChange struct {
	Path     string
	Original []byte
	Updated  []byte
	Options  FlameworkOptions
	Status   TSConfigChangeStatus
}

type JSONCMigrationError struct {
	Path   string
	Reason string
	Cause  error
	Count  int
}

func (e *JSONCMigrationError) Error() string {
	return fmt.Sprintf("migrate %s: %s", e.Path, e.Reason)
}

func (e *JSONCMigrationError) Unwrap() error { return e.Cause }

var (
	ErrNoFlameworkPlugin        = errors.New("no Flamework plugin")
	ErrMultipleFlameworkPlugins = errors.New("multiple Flamework plugins")
)

type jsoncKind uint8

const (
	jsoncObject jsoncKind = iota
	jsoncArray
	jsoncString
	jsoncNumber
	jsoncBool
	jsoncNull
)

type jsoncProperty struct {
	key   string
	value *jsoncNode
}

type jsoncNode struct {
	kind          jsoncKind
	start         int
	end           int
	properties    []jsoncProperty
	elements      []*jsoncNode
	commas        []int
	trailingComma bool
}

type jsoncDocument struct {
	path string
	raw  []byte
	root *jsoncNode
}

type jsoncParser struct {
	path string
	src  []byte
	pos  int
}

type effectivePlugins struct {
	document *jsoncDocument
	array    *jsoncNode
	plugins  []pluginEntry
	declared bool
}

type pluginEntry struct {
	transform string
	node      *jsoncNode
	document  *jsoncDocument
}

// PlanFlameworkTSConfig plans, but never writes, the hard-cut migration for an
// explicit tsconfig file. Extends are confined to the target file's directory.
func PlanFlameworkTSConfig(path string) (TSConfigChange, error) {
	absPath, err := filepath.Abs(path)
	if err != nil {
		return TSConfigChange{}, &JSONCMigrationError{Path: path, Reason: fmt.Sprintf("resolve target: %v", err)}
	}
	documents := make(map[string]*jsoncDocument)
	effective, err := resolveEffectivePlugins(absPath, documents, make(map[string]bool))
	if err != nil {
		return TSConfigChange{}, err
	}
	if !effective.declared {
		return TSConfigChange{}, flameworkCountError(absPath, 0)
	}

	flameworkIndex := -1
	for index, plugin := range effective.plugins {
		if plugin.transform == flameworkTransformer {
			if flameworkIndex >= 0 {
				count := 0
				for _, candidate := range effective.plugins {
					if candidate.transform == flameworkTransformer {
						count++
					}
				}
				return TSConfigChange{}, flameworkCountError(absPath, count)
			}
			flameworkIndex = index
		}
	}
	if flameworkIndex < 0 {
		return TSConfigChange{}, flameworkCountError(absPath, 0)
	}
	if err := rejectDuplicateTransforms(absPath, effective.plugins); err != nil {
		return TSConfigChange{}, err
	}

	options, err := parseFlameworkOptions(effective.plugins[flameworkIndex])
	if err != nil {
		return TSConfigChange{}, err
	}
	remaining := append([]pluginEntry(nil), effective.plugins[:flameworkIndex]...)
	remaining = append(remaining, effective.plugins[flameworkIndex+1:]...)
	if flameworkIndex > 0 {
		options.After = effective.plugins[flameworkIndex-1].transform
	}

	target := documents[absPath]
	flameworkPlugin := effective.plugins[flameworkIndex]
	updated, err := rewriteTargetPlugins(target, remaining, flameworkPlugin, effective.document.path == target.path)
	if err != nil {
		return TSConfigChange{}, err
	}
	return TSConfigChange{
		Path: absPath, Original: bytes.Clone(target.raw), Updated: updated,
		Options: options, Status: TSConfigChangePlanned,
	}, nil
}

func flameworkCountError(path string, count int) error {
	if count == 0 {
		return &JSONCMigrationError{Path: path, Reason: "expected exactly one rbxts-transformer-flamework plugin, found 0", Cause: ErrNoFlameworkPlugin, Count: count}
	}
	return &JSONCMigrationError{Path: path, Reason: fmt.Sprintf("expected exactly one rbxts-transformer-flamework plugin, found %d", count), Cause: ErrMultipleFlameworkPlugins, Count: count}
}

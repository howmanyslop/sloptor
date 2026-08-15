package migrate

import (
	"errors"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"

	"github.com/BurntSushi/toml"

	"rotor/internal/config"
)

// FlameworkOptions are the native settings translated from the legacy plugin.
type FlameworkOptions struct {
	After, IDGenerationMode, HashPrefix, Salt      string
	NoSemanticDiagnostics, Obfuscation, PreloadIDs bool
	SkipUnchangedFiles                             bool
	Optimizations                                  FlameworkOptimizations
}

type FlameworkOptimizations struct{ GuardGenerationDedupLimit *int }

type MergeStatus uint8

const (
	MergeReady MergeStatus = iota
	MergeAlreadyMigrated
	MergeInvalid
)

type MergeErrorKind uint8

const (
	MergeErrorAlreadyMigrated MergeErrorKind = iota + 1
	MergeErrorInvalidTOML
	MergeErrorRead
)

// MergeError identifies a preflight failure callers can present without parsing text.
type MergeError struct {
	Path string
	Kind MergeErrorKind
	Err  error
}

func (e *MergeError) Error() string { return fmt.Sprintf("merge %s: %v", e.Path, e.Err) }
func (e *MergeError) Unwrap() error { return e.Err }

// MergeFlameworkTOML appends native configuration without reserializing existing TOML.
func MergeFlameworkTOML(path string, options FlameworkOptions) (FileChange, MergeStatus, error) {
	return mergeFlameworkTOML(path, renderFlamework(options))
}

func MergeFlameworkProfilesTOML(path string, profiles map[string]FlameworkOptions) (FileChange, MergeStatus, error) {
	return mergeFlameworkTOML(path, renderFlameworkProfiles(profiles))
}

func mergeFlameworkTOML(path, rendered string) (FileChange, MergeStatus, error) {
	original, err := os.ReadFile(path)
	if errors.Is(err, fs.ErrNotExist) {
		updated := []byte(config.SchemaDirective + "\n\n" + rendered)
		return FileChange{Path: path, Updated: updated}, MergeReady, nil
	}
	if err != nil {
		return FileChange{}, MergeInvalid, &MergeError{Path: path, Kind: MergeErrorRead, Err: err}
	}
	var document map[string]any
	if _, err := toml.Decode(string(original), &document); err != nil {
		return FileChange{}, MergeInvalid, &MergeError{Path: path, Kind: MergeErrorInvalidTOML, Err: err}
	}
	if _, exists := document["flamework"]; exists {
		return FileChange{}, MergeAlreadyMigrated, &MergeError{
			Path: path, Kind: MergeErrorAlreadyMigrated, Err: errors.New("[flamework] already exists"),
		}
	}
	updated := appendFlameworkTable(original, rendered)
	return FileChange{Path: path, Original: original, Updated: updated, Existed: true}, MergeReady, nil
}

func renderFlameworkProfiles(profiles map[string]FlameworkOptions) string {
	names := make([]string, 0, len(profiles))
	for name := range profiles {
		names = append(names, name)
	}
	sort.Strings(names)
	var rendered strings.Builder
	for index, name := range names {
		if index > 0 {
			rendered.WriteByte('\n')
		}
		prefix := "flamework.profiles." + quoteTOML(filepath.ToSlash(name))
		rendered.WriteString(renderFlameworkTables("["+prefix+"]", "["+prefix+".optimizations]", profiles[name]))
	}
	return rendered.String()
}

func ExistingFlameworkOptions(path string) (FlameworkOptions, bool, error) {
	project, err := config.Load(filepath.Dir(path))
	if errors.Is(err, config.ErrNotFound) {
		return FlameworkOptions{}, false, nil
	}
	if err != nil {
		return FlameworkOptions{}, false, err
	}
	if project.Flamework == nil || len(project.Flamework.Profiles) > 0 {
		return FlameworkOptions{}, false, nil
	}
	return FlameworkOptions{
		After:                 project.Flamework.After,
		NoSemanticDiagnostics: project.Flamework.NoSemanticDiagnostics,
		Obfuscation:           project.Flamework.Obfuscation,
		IDGenerationMode:      project.Flamework.IDGenerationMode,
		HashPrefix:            project.Flamework.HashPrefix,
		Salt:                  project.Flamework.Salt,
		PreloadIDs:            project.Flamework.PreloadIDs,
		SkipUnchangedFiles:    project.Flamework.SkipUnchangedFiles,
		Optimizations: FlameworkOptimizations{
			GuardGenerationDedupLimit: project.Flamework.Optimizations.GuardGenerationDedupLimit,
		},
	}, true, nil
}

func renderFlamework(options FlameworkOptions) string {
	return renderFlameworkTables("[flamework]", "[flamework.optimizations]", options)
}

func renderFlameworkTables(header, optimizationsHeader string, options FlameworkOptions) string {
	lines := []string{header}
	for _, option := range []struct{ key, value string }{
		{"after", options.After},
		{"idGenerationMode", options.IDGenerationMode},
		{"hashPrefix", options.HashPrefix},
		{"salt", options.Salt},
	} {
		if option.value != "" {
			lines = append(lines, option.key+" = "+quoteTOML(option.value))
		}
	}
	for _, option := range []struct {
		key     string
		enabled bool
	}{
		{"noSemanticDiagnostics", options.NoSemanticDiagnostics}, {"obfuscation", options.Obfuscation}, {"preloadIds", options.PreloadIDs}, {"skipUnchangedFiles", options.SkipUnchangedFiles},
	} {
		if option.enabled {
			lines = append(lines, option.key+" = true")
		}
	}
	text := strings.Join(lines, "\n") + "\n"
	if limit := options.Optimizations.GuardGenerationDedupLimit; limit != nil {
		text += "\n" + optimizationsHeader + "\nguardGenerationDedupLimit = " + strconv.Itoa(*limit) + "\n"
	}
	return text
}

func quoteTOML(value string) string {
	var quoted strings.Builder
	quoted.Grow(len(value) + 2)
	quoted.WriteByte('"')
	for _, character := range value {
		switch character {
		case '\\':
			quoted.WriteString(`\\`)
		case '"':
			quoted.WriteString(`\"`)
		case '\b':
			quoted.WriteString(`\b`)
		case '\t':
			quoted.WriteString(`\t`)
		case '\n':
			quoted.WriteString(`\n`)
		case '\f':
			quoted.WriteString(`\f`)
		case '\r':
			quoted.WriteString(`\r`)
		default:
			if character < 0x20 || character == 0x7f {
				fmt.Fprintf(&quoted, `\u%04X`, character)
			} else {
				quoted.WriteRune(character)
			}
		}
	}
	quoted.WriteByte('"')
	return quoted.String()
}

func appendFlameworkTable(original []byte, table string) []byte {
	updated := append([]byte(nil), original...)
	if len(updated) != 0 {
		if updated[len(updated)-1] == '\n' {
			updated = append(updated, '\n')
		} else {
			updated = append(updated, '\n', '\n')
		}
	}
	return append(updated, table...)
}

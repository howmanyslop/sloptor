package flamework

import (
	"encoding/json"
	"errors"
	"fmt"
	"os"
)

const BuildInfoSchemaVersion = 1

var (
	ErrInvalidBuildInfo    = errors.New("flamework: invalid flamework.build")
	ErrDuplicateIdentifier = errors.New("flamework: duplicate identifier")
	ErrDuplicateBuildClass = errors.New("flamework: duplicate build class")
)

type BuildDecorator struct {
	Name       string `json:"name,omitempty"`
	InternalID string `json:"internalId,omitempty"`
}

type BuildClass struct {
	FilePath   string           `json:"filePath,omitempty"`
	InternalID string           `json:"internalId,omitempty"`
	Decorators []BuildDecorator `json:"decorators,omitempty"`
}

type BuildGlobs struct {
	Paths   *map[string][]string `json:"paths,omitempty"`
	Origins *map[string][]string `json:"origins,omitempty"`
}

type BuildMetadata struct {
	Config *RuntimeConfig `json:"config,omitempty"`
	Globs  *BuildGlobs    `json:"globs,omitempty"`
}

type BuildInfoSnapshot struct {
	Path             string
	Version          float64
	FlameworkVersion string
	Prefix           *string
	Salt             *string
	Metadata         *BuildMetadata
	StringHashes     map[string]string
	Identifiers      map[string]string
	Classes          []BuildClass
}

func (s BuildInfoSnapshot) IdentifierPrefix() (string, bool) {
	if s.Prefix == nil {
		return "", false
	}
	return *s.Prefix, true
}

type BuildInfo struct {
	path               string
	version            float64
	flameworkVersion   string
	prefix             *string
	salt               *string
	metadata           *BuildMetadata
	stringHashes       *orderedStringMap
	identifiers        orderedStringMap
	classes            *[]BuildClass
	fieldOrder         []string
	extras             map[string]json.RawMessage
	packages           []*BuildInfo
	reverseIdentifiers map[string]string
}

func NewBuildInfo(path, flameworkVersion string) *BuildInfo {
	return &BuildInfo{
		path: path, version: BuildInfoSchemaVersion, flameworkVersion: flameworkVersion,
		identifiers: newOrderedStringMap(), fieldOrder: []string{"version", "flameworkVersion", "identifiers"},
		extras: map[string]json.RawMessage{}, reverseIdentifiers: map[string]string{},
	}
}

func LoadBuildInfo(path, flameworkVersion string) (*BuildInfo, error) {
	data, err := os.ReadFile(path)
	if errors.Is(err, os.ErrNotExist) {
		return NewBuildInfo(path, flameworkVersion), nil
	}
	if err != nil {
		return nil, fmt.Errorf("flamework: read build info: %w", err)
	}
	info := NewBuildInfo(path, flameworkVersion)
	if err := info.decode(data); err != nil {
		return nil, err
	}
	return info, nil
}

func (b *BuildInfo) Path() string             { return b.path }
func (b *BuildInfo) FlameworkVersion() string { return b.flameworkVersion }
func (b *BuildInfo) LatestID() int            { return len(b.identifiers.order) + 1 }

func (b *BuildInfo) Prefix() (string, bool) {
	if b.prefix == nil {
		return "", false
	}
	return *b.prefix, true
}

func (b *BuildInfo) Salt() (string, bool) {
	if b.salt == nil {
		return "", false
	}
	return *b.salt, true
}

func (b *BuildInfo) SetIdentifierPrefix(prefix *string) {
	b.prefix = cloneStringPointer(prefix)
	b.ensureField("identifierPrefix")
}

func (b *BuildInfo) SetSalt(salt *string) {
	b.salt = cloneStringPointer(salt)
	b.ensureField("salt")
}

func (b *BuildInfo) AddIdentifier(internalID, id string) error {
	if _, ok := b.Identifier(internalID); ok {
		return fmt.Errorf("%w: %s", ErrDuplicateIdentifier, internalID)
	}
	b.identifiers.set(internalID, id)
	b.reverseIdentifiers[id] = internalID
	return nil
}

func (b *BuildInfo) StringHash(key string) (string, bool) {
	if b.stringHashes == nil {
		return "", false
	}
	value, ok := b.stringHashes.values[key]
	return value, ok
}

func (b *BuildInfo) SetStringHash(key, value string) {
	if b.stringHashes == nil {
		hashes := newOrderedStringMap()
		b.stringHashes = &hashes
		b.ensureField("stringHashes")
	}
	b.stringHashes.set(key, value)
}

func (b *BuildInfo) HashString(text, context string) (string, error) {
	key := context + ":" + text
	if value, ok := b.StringHash(key); ok {
		return value, nil
	}
	value, err := NewUUIDv4()
	if err != nil {
		return "", fmt.Errorf("flamework: hash string: %w", err)
	}
	b.SetStringHash(key, value)
	return value, nil
}

func (b *BuildInfo) Identifier(internalID string) (string, bool) {
	if id, ok := b.identifiers.values[internalID]; ok {
		return id, true
	}
	for _, child := range b.packages {
		if id, ok := child.Identifier(internalID); ok {
			return id, true
		}
	}
	return "", false
}

func (b *BuildInfo) InternalIdentifier(id string) (string, bool) {
	if internalID, ok := b.reverseIdentifiers[id]; ok {
		return internalID, true
	}
	for _, child := range b.packages {
		if identifier, ok := child.Identifier(id); ok {
			return identifier, true
		}
	}
	return "", false
}

func (b *BuildInfo) AddClass(class BuildClass) error {
	if _, ok := b.Class(class.InternalID); ok {
		return fmt.Errorf("%w: %s", ErrDuplicateBuildClass, class.InternalID)
	}
	if b.classes == nil {
		classes := []BuildClass{}
		b.classes = &classes
		b.ensureField("classes")
	}
	*b.classes = append(*b.classes, cloneBuildClass(class))
	return nil
}

func (b *BuildInfo) Class(internalID string) (BuildClass, bool) {
	if b.classes != nil {
		for _, class := range *b.classes {
			if class.InternalID == internalID {
				return cloneBuildClass(class), true
			}
		}
	}
	for _, child := range b.packages {
		if class, ok := child.Class(internalID); ok {
			return class, true
		}
	}
	return BuildClass{}, false
}

func (b *BuildInfo) AddPackage(child *BuildInfo) error {
	if child == nil {
		return fmt.Errorf("%w: nil package", ErrInvalidBuildInfo)
	}
	b.packages = append(b.packages, child)
	return nil
}

func (b *BuildInfo) ensureField(name string) {
	for _, field := range b.fieldOrder {
		if field == name {
			return
		}
	}
	b.fieldOrder = append(b.fieldOrder, name)
}

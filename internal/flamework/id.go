package flamework

import (
	"crypto/rand"
	"encoding/hex"
	"errors"
	"fmt"
	"io"
	"strings"
)

const DefaultHashContext = "@"

var (
	ErrInvalidIDMode   = errors.New("invalid Flamework ID generation mode")
	ErrNilStringHashes = errors.New("flamework string hash map is nil")
)

type IdentifierRequest struct {
	Mode            IDMode
	Salt            string
	HashPrefix      string
	PackageName     string
	InternalID      string
	DeclarationName string
	LuaFileName     string
	PackagePrefix   string
	NextID          uint64
	IsGame          bool
	IsPackage       bool
}

// GenerateIdentifier reproduces the stable identifier branches in Flamework 1.3.2 uid.ts.
func GenerateIdentifier(request IdentifierRequest) (string, error) {
	if request.IsPackage {
		if request.PackagePrefix == "" {
			return request.InternalID, nil
		}
		return formatInternalID(request.InternalID, request.PackagePrefix), nil
	}

	switch request.Mode {
	case IDModeFull:
		return formatInternalID(request.InternalID, request.HashPrefix), nil
	case IDModeObfuscated:
		return generateHashedIdentifier(request, false)
	case IDModeShort, IDModeTiny:
		hash, err := generateHashedIdentifier(request, true)
		if err != nil {
			return "", err
		}
		name := request.DeclarationName + "{" + hash + "}"
		if request.Mode == IDModeShort {
			name = request.LuaFileName + "@" + name
		}
		if request.HashPrefix != "" {
			name = request.HashPrefix + ":" + name
		}
		return name, nil
	default:
		return "", fmt.Errorf("%w %q", ErrInvalidIDMode, request.Mode)
	}
}

func generateHashedIdentifier(request IdentifierRequest, omitPrefix bool) (string, error) {
	hash, err := EncodeHashID(request.Salt, request.NextID)
	if err != nil {
		return "", err
	}
	if omitPrefix || (request.IsGame && request.HashPrefix == "") {
		return hash, nil
	}
	prefix := request.HashPrefix
	if prefix == "" {
		prefix = request.PackageName
	}
	return prefix + ":" + hash, nil
}

func formatInternalID(internalID, hashPrefix string) string {
	colon := strings.LastIndexByte(internalID, ':')
	at := strings.LastIndexByte(internalID, '@')
	if colon < 0 || at <= colon+1 || at == len(internalID)-1 {
		return internalID
	}
	path := internalID[colon+1 : at]
	if separator := strings.IndexAny(path, `/\`); separator >= 0 {
		path = path[separator+1:]
	}
	formatted := path + "@" + internalID[at+1:]
	if hashPrefix != "" {
		formatted = hashPrefix + ":" + formatted
	}
	return formatted
}

// NewSalt returns the lowercase hex encoding of 64 cryptographically random bytes.
func NewSalt() (string, error) {
	return newSalt(rand.Reader)
}

func newSalt(reader io.Reader) (string, error) {
	bytes := make([]byte, 64)
	if _, err := io.ReadFull(reader, bytes); err != nil {
		return "", fmt.Errorf("generate Flamework salt: %w", err)
	}
	return hex.EncodeToString(bytes), nil
}

// NewUUIDv4 returns a cryptographically random RFC 4122 version 4 UUID.
func NewUUIDv4() (string, error) {
	return newUUIDV4(rand.Reader)
}

func newUUIDV4(reader io.Reader) (string, error) {
	var uuid [16]byte
	if _, err := io.ReadFull(reader, uuid[:]); err != nil {
		return "", fmt.Errorf("generate Flamework UUID: %w", err)
	}
	uuid[6] = uuid[6]&0x0f | 0x40
	uuid[8] = uuid[8]&0x3f | 0x80
	return fmt.Sprintf("%08x-%04x-%04x-%04x-%012x",
		uuid[0:4], uuid[4:6], uuid[6:8], uuid[8:10], uuid[10:16]), nil
}

// StableStringHash returns the persisted UUID for context:text or creates one.
func StableStringHash(hashes map[string]string, text, context string) (string, error) {
	return stableStringHash(hashes, text, context, rand.Reader)
}

func stableStringHash(hashes map[string]string, text, context string, reader io.Reader) (string, error) {
	if hashes == nil {
		return "", ErrNilStringHashes
	}
	key := context + ":" + text
	if hash := hashes[key]; hash != "" {
		return hash, nil
	}
	hash, err := newUUIDV4(reader)
	if err != nil {
		return "", err
	}
	hashes[key] = hash
	return hash, nil
}

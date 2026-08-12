package flamework

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"io"
)

type orderedStringMap struct {
	order  []string
	values map[string]string
}

func newOrderedStringMap() orderedStringMap {
	return orderedStringMap{values: map[string]string{}}
}

func (m *orderedStringMap) set(key, value string) {
	if _, ok := m.values[key]; !ok {
		m.order = append(m.order, key)
	}
	m.values[key] = value
}

func (m orderedStringMap) MarshalJSON() ([]byte, error) {
	buffer := bytes.NewBufferString("{")
	for index, key := range m.order {
		if index > 0 {
			buffer.WriteByte(',')
		}
		encodedKey, err := marshalJSONNoEscape(key)
		if err != nil {
			return nil, err
		}
		encodedValue, err := marshalJSONNoEscape(m.values[key])
		if err != nil {
			return nil, err
		}
		buffer.Write(encodedKey)
		buffer.WriteByte(':')
		buffer.Write(encodedValue)
	}
	buffer.WriteByte('}')
	return buffer.Bytes(), nil
}

func (b *BuildInfo) MarshalOrderedJSON() ([]byte, error) {
	compact := bytes.NewBufferString("{")
	written := 0
	for _, name := range b.fieldOrder {
		value, present, err := b.fieldJSON(name)
		if err != nil {
			return nil, fmt.Errorf("flamework: encode build info %s: %w", name, err)
		}
		if !present {
			continue
		}
		if written > 0 {
			compact.WriteByte(',')
		}
		key, err := marshalJSONNoEscape(name)
		if err != nil {
			return nil, fmt.Errorf("flamework: encode build info key: %w", err)
		}
		compact.Write(key)
		compact.WriteByte(':')
		compact.Write(value)
		written++
	}
	compact.WriteByte('}')
	var indented bytes.Buffer
	if err := json.Indent(&indented, compact.Bytes(), "", "\t"); err != nil {
		return nil, fmt.Errorf("flamework: indent build info: %w", err)
	}
	return indented.Bytes(), nil
}

func (b *BuildInfo) decode(data []byte) error {
	if err := validateJSONSyntax(data); err != nil {
		return fmt.Errorf("%w: %w", ErrInvalidBuildInfo, err)
	}
	decoder := json.NewDecoder(bytes.NewReader(data))
	token, err := decoder.Token()
	if err != nil || token != json.Delim('{') {
		return fmt.Errorf("%w: expected object", ErrInvalidBuildInfo)
	}
	b.fieldOrder = nil
	b.identifiers = newOrderedStringMap()
	b.reverseIdentifiers = map[string]string{}
	required := map[string]bool{"version": false, "flameworkVersion": false, "identifiers": false}
	for decoder.More() {
		nameToken, tokenErr := decoder.Token()
		if tokenErr != nil {
			return fmt.Errorf("%w: %v", ErrInvalidBuildInfo, tokenErr)
		}
		name, ok := nameToken.(string)
		if !ok {
			return fmt.Errorf("%w: non-string property", ErrInvalidBuildInfo)
		}
		var raw json.RawMessage
		if err := decoder.Decode(&raw); err != nil {
			return fmt.Errorf("%w: %v", ErrInvalidBuildInfo, err)
		}
		b.ensureField(name)
		if _, ok := required[name]; ok {
			required[name] = true
		}
		if err := b.decodeField(name, raw); err != nil {
			return err
		}
	}
	if _, err := decoder.Token(); err != nil {
		return fmt.Errorf("%w: %v", ErrInvalidBuildInfo, err)
	}
	if token, err := decoder.Token(); !errors.Is(err, io.EOF) || token != nil {
		return fmt.Errorf("%w: trailing JSON value", ErrInvalidBuildInfo)
	}
	for name, present := range required {
		if !present {
			return fmt.Errorf("%w: missing %s", ErrInvalidBuildInfo, name)
		}
	}
	return nil
}

func (b *BuildInfo) decodeField(name string, raw json.RawMessage) error {
	switch name {
	case "version":
		return decodeBuildField(raw, name, &b.version)
	case "flameworkVersion":
		return decodeBuildField(raw, name, &b.flameworkVersion)
	case "identifierPrefix":
		return decodeBuildField(raw, name, &b.prefix)
	case "salt":
		return decodeBuildField(raw, name, &b.salt)
	case "metadata":
		metadata, err := decodeBuildMetadata(raw)
		b.metadata = metadata
		return err
	case "classes":
		return decodeBuildField(raw, name, &b.classes)
	case "identifiers":
		values, err := decodeOrderedStringMap(raw)
		if err != nil {
			return err
		}
		b.identifiers = values
		for internalID, id := range values.values {
			b.reverseIdentifiers[id] = internalID
		}
		return nil
	case "stringHashes":
		values, err := decodeOrderedStringMap(raw)
		if err != nil {
			return err
		}
		b.stringHashes = &values
		return nil
	default:
		b.extras[name] = append(json.RawMessage(nil), raw...)
		return nil
	}
}

func decodeBuildField(raw json.RawMessage, name string, target any) error {
	if bytes.Equal(bytes.TrimSpace(raw), []byte("null")) {
		return fmt.Errorf("%w: %s must not be null", ErrInvalidBuildInfo, name)
	}
	if err := json.Unmarshal(raw, target); err != nil {
		return fmt.Errorf("%w: %s: %v", ErrInvalidBuildInfo, name, err)
	}
	return nil
}

func decodeBuildMetadata(raw []byte) (*BuildMetadata, error) {
	var properties map[string]json.RawMessage
	if len(bytes.TrimSpace(raw)) == 0 || bytes.Equal(bytes.TrimSpace(raw), []byte("null")) || json.Unmarshal(raw, &properties) != nil || properties == nil {
		return nil, fmt.Errorf("%w: metadata must be object", ErrInvalidBuildInfo)
	}
	metadata := &BuildMetadata{}
	if configRaw, ok := properties["config"]; ok {
		config, err := parseRuntimeConfig(configRaw)
		if err != nil {
			return nil, fmt.Errorf("%w: metadata config: %v", ErrInvalidBuildInfo, err)
		}
		metadata.Config = &config
	}
	if globsRaw, ok := properties["globs"]; ok {
		if bytes.Equal(bytes.TrimSpace(globsRaw), []byte("null")) || json.Unmarshal(globsRaw, &metadata.Globs) != nil || metadata.Globs == nil {
			return nil, fmt.Errorf("%w: metadata globs must be object", ErrInvalidBuildInfo)
		}
	}
	return metadata, nil
}

func decodeOrderedStringMap(raw []byte) (orderedStringMap, error) {
	result := newOrderedStringMap()
	decoder := json.NewDecoder(bytes.NewReader(raw))
	token, err := decoder.Token()
	if err != nil || token != json.Delim('{') {
		return result, fmt.Errorf("%w: expected string map", ErrInvalidBuildInfo)
	}
	for decoder.More() {
		keyToken, err := decoder.Token()
		if err != nil {
			return result, fmt.Errorf("%w: %v", ErrInvalidBuildInfo, err)
		}
		key, ok := keyToken.(string)
		if !ok {
			return result, fmt.Errorf("%w: non-string map key", ErrInvalidBuildInfo)
		}
		var value string
		if err := decoder.Decode(&value); err != nil {
			return result, fmt.Errorf("%w: string map value: %v", ErrInvalidBuildInfo, err)
		}
		result.set(key, value)
	}
	if _, err := decoder.Token(); err != nil {
		return result, fmt.Errorf("%w: %v", ErrInvalidBuildInfo, err)
	}
	return result, nil
}

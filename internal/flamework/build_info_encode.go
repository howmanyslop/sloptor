package flamework

import (
	"bytes"
	"encoding/json"
)

func (b *BuildInfo) fieldJSON(name string) ([]byte, bool, error) {
	var value any
	switch name {
	case "version":
		value = b.version
	case "flameworkVersion":
		value = b.flameworkVersion
	case "identifierPrefix":
		if b.prefix == nil {
			return nil, false, nil
		}
		value = *b.prefix
	case "salt":
		if b.salt == nil {
			return nil, false, nil
		}
		value = *b.salt
	case "metadata":
		if b.metadata == nil {
			return nil, false, nil
		}
		value = b.metadata
	case "stringHashes":
		if b.stringHashes == nil {
			return nil, false, nil
		}
		value = b.stringHashes
	case "identifiers":
		value = b.identifiers
	case "classes":
		if b.classes == nil {
			return nil, false, nil
		}
		value = *b.classes
	default:
		raw, ok := b.extras[name]
		return append([]byte(nil), raw...), ok, nil
	}
	data, err := marshalJSONNoEscape(value)
	return data, true, err
}

func marshalJSONNoEscape(value any) ([]byte, error) {
	var buffer bytes.Buffer
	encoder := json.NewEncoder(&buffer)
	encoder.SetEscapeHTML(false)
	if err := encoder.Encode(value); err != nil {
		return nil, err
	}
	return bytes.TrimSuffix(buffer.Bytes(), []byte{'\n'}), nil
}

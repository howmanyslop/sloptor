package main

import (
	"encoding/hex"
	"fmt"
)

// Inspect every raw string, including unknown keys and values, before Token or
// Unmarshal can replace unpaired UTF-16 surrogates with a valid U+FFFD. Other
// JSON syntax remains the decoder's responsibility. Byte offsets are zero-based.
func (v *validation) unicodeEscapes(input []byte) bool {
	valid := true
	for i := 0; i < len(input); i++ {
		if input[i] != '"' {
			continue
		}
		for i++; i < len(input) && input[i] != '"'; {
			if input[i] != '\\' {
				i++
				continue
			}
			if i+1 >= len(input) || input[i+1] != 'u' {
				i += 2 // Skip escaped backslashes/quotes without treating their text as escapes.
				continue
			}
			unit, err := escapedUnicodeUnit(input, i)
			if err != nil {
				v.invalid("$", err.Error())
				return false
			}
			switch {
			case unit >= 0xd800 && unit <= 0xdbff:
				if i+7 < len(input) && input[i+6] == '\\' && input[i+7] == 'u' {
					low, err := escapedUnicodeUnit(input, i+6)
					if err != nil {
						v.invalid("$", err.Error())
						return false
					}
					if low >= 0xdc00 && low <= 0xdfff {
						i += 12
						continue
					}
				}
				v.invalid("$", fmt.Sprintf("unpaired high surrogate escape at byte %d; require an immediately following low surrogate escape", i))
				valid = false
			case unit >= 0xdc00 && unit <= 0xdfff:
				v.invalid("$", fmt.Sprintf("unpaired low surrogate escape at byte %d", i))
				valid = false
			}
			i += 6
		}
	}
	return valid
}

func escapedUnicodeUnit(input []byte, start int) (uint16, error) {
	var unit [2]byte
	if start+6 <= len(input) {
		if _, err := hex.Decode(unit[:], input[start+2:start+6]); err == nil {
			return uint16(unit[0])<<8 | uint16(unit[1]), nil
		}
	}
	return 0, fmt.Errorf("invalid Unicode escape at byte %d; require four hexadecimal digits", start)
}

package migrate

import (
	"bytes"
	"encoding/json"
	"fmt"
	"strings"
)

func (p *jsoncParser) parseValue() (*jsoncNode, error) {
	if err := p.skipTrivia(); err != nil {
		return nil, err
	}
	if p.pos >= len(p.src) {
		return nil, p.failure("unexpected end of file")
	}
	switch p.src[p.pos] {
	case '{':
		return p.parseObject()
	case '[':
		return p.parseArray()
	case '"':
		return p.parseStringNode()
	case 't':
		return p.parseLiteral("true", jsoncBool)
	case 'f':
		return p.parseLiteral("false", jsoncBool)
	case 'n':
		return p.parseLiteral("null", jsoncNull)
	default:
		return p.parseNumber()
	}
}

func (p *jsoncParser) parseObject() (*jsoncNode, error) {
	node := &jsoncNode{kind: jsoncObject, start: p.pos}
	p.pos++
	if err := p.skipTrivia(); err != nil {
		return nil, err
	}
	seen := make(map[string]bool)
	for p.pos < len(p.src) && p.src[p.pos] != '}' {
		keyNode, err := p.parseStringNode()
		if err != nil {
			return nil, err
		}
		key, err := decodeRawString(p.src[keyNode.start:keyNode.end])
		if err != nil {
			return nil, p.failure("invalid object key")
		}
		if seen[key] {
			return nil, p.failure(fmt.Sprintf("duplicate JSONC key %q", key))
		}
		seen[key] = true
		if err := p.skipTrivia(); err != nil {
			return nil, err
		}
		if !p.consume(':') {
			return nil, p.failure("expected ':' after object key")
		}
		value, err := p.parseValue()
		if err != nil {
			return nil, err
		}
		node.properties = append(node.properties, jsoncProperty{key: key, value: value})
		if err := p.skipTrivia(); err != nil {
			return nil, err
		}
		if p.consume(',') {
			if err := p.skipTrivia(); err != nil {
				return nil, err
			}
			if p.pos < len(p.src) && p.src[p.pos] == '}' {
				node.trailingComma = true
				break
			}
			continue
		}
		break
	}
	if !p.consume('}') {
		return nil, p.failure("expected '}'")
	}
	node.end = p.pos
	return node, nil
}

func (p *jsoncParser) parseArray() (*jsoncNode, error) {
	node := &jsoncNode{kind: jsoncArray, start: p.pos}
	p.pos++
	if err := p.skipTrivia(); err != nil {
		return nil, err
	}
	for p.pos < len(p.src) && p.src[p.pos] != ']' {
		value, err := p.parseValue()
		if err != nil {
			return nil, err
		}
		node.elements = append(node.elements, value)
		if err := p.skipTrivia(); err != nil {
			return nil, err
		}
		if p.consume(',') {
			node.commas = append(node.commas, p.pos-1)
			if err := p.skipTrivia(); err != nil {
				return nil, err
			}
			if p.pos < len(p.src) && p.src[p.pos] == ']' {
				node.trailingComma = true
				break
			}
			continue
		}
		break
	}
	if !p.consume(']') {
		return nil, p.failure("expected ']' ")
	}
	node.end = p.pos
	return node, nil
}

func (p *jsoncParser) parseStringNode() (*jsoncNode, error) {
	if !p.consume('"') {
		return nil, p.failure("expected string")
	}
	start := p.pos - 1
	for p.pos < len(p.src) {
		switch p.src[p.pos] {
		case '\\':
			p.pos += 2
		case '"':
			p.pos++
			if _, err := decodeRawString(p.src[start:p.pos]); err != nil {
				return nil, p.failure("invalid string")
			}
			return &jsoncNode{kind: jsoncString, start: start, end: p.pos}, nil
		default:
			p.pos++
		}
	}
	return nil, p.failure("unterminated string")
}

func (p *jsoncParser) parseLiteral(literal string, kind jsoncKind) (*jsoncNode, error) {
	start := p.pos
	if !bytes.HasPrefix(p.src[p.pos:], []byte(literal)) {
		return nil, p.failure("invalid JSON value")
	}
	p.pos += len(literal)
	return &jsoncNode{kind: kind, start: start, end: p.pos}, nil
}

func (p *jsoncParser) parseNumber() (*jsoncNode, error) {
	start := p.pos
	for p.pos < len(p.src) && strings.ContainsRune("-+0123456789.eE", rune(p.src[p.pos])) {
		p.pos++
	}
	if start == p.pos {
		return nil, p.failure("invalid JSON value")
	}
	var number json.Number
	if err := json.Unmarshal(p.src[start:p.pos], &number); err != nil {
		return nil, p.failure("invalid number")
	}
	return &jsoncNode{kind: jsoncNumber, start: start, end: p.pos}, nil
}

func (p *jsoncParser) skipTrivia() error {
	for p.pos < len(p.src) {
		switch {
		case strings.ContainsRune(" \t\r\n", rune(p.src[p.pos])):
			p.pos++
		case p.pos+1 < len(p.src) && p.src[p.pos] == '/' && p.src[p.pos+1] == '/':
			p.pos += 2
			for p.pos < len(p.src) && p.src[p.pos] != '\n' {
				p.pos++
			}
		case p.pos+1 < len(p.src) && p.src[p.pos] == '/' && p.src[p.pos+1] == '*':
			commentStart := p.pos
			p.pos += 2
			for p.pos+1 < len(p.src) && (p.src[p.pos] != '*' || p.src[p.pos+1] != '/') {
				p.pos++
			}
			if p.pos+1 < len(p.src) {
				p.pos += 2
			} else {
				p.pos = commentStart
				return p.failure("unterminated block comment")
			}
		default:
			return nil
		}
	}
	return nil
}

func (p *jsoncParser) consume(value byte) bool {
	if p.pos >= len(p.src) || p.src[p.pos] != value {
		return false
	}
	p.pos++
	return true
}

func (p *jsoncParser) failure(reason string) error {
	if p.pos+1 < len(p.src) && p.src[p.pos] == '/' && p.src[p.pos+1] == '*' && !bytes.Contains(p.src[p.pos+2:], []byte("*/")) {
		reason = "unterminated block comment"
	}
	return &JSONCMigrationError{Path: p.path, Reason: fmt.Sprintf("invalid JSONC at byte %d: %s", p.pos, reason)}
}

func decodeRawString(raw []byte) (string, error) {
	var value string
	err := json.Unmarshal(raw, &value)
	return value, err
}

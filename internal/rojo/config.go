// Rojo project-file parsing: an insertion-ordered JSON reader plus a hand
// validator mirroring rojo-schema.json (upstream validates with ajv;
// reference/rojo-resolver/src/RojoResolver.ts L148-152). Key order matters
// because parseTree walks Object.keys(tree), which drives partition LIFO
// ordering, so encoding/json's unordered maps cannot be used.
package rojo

import (
	"bytes"
	"encoding/json"
	"io"
	"math"
	"strconv"
	"strings"
)

// Tree is a parsed Rojo project tree node (RojoResolver.ts RojoTree,
// L21-37). Exported so FromTree callers can build trees programmatically
// (upstream fromTree's only call site is the playground VirtualProject).
type Tree struct {
	// ClassName is $className ("" when absent).
	ClassName string
	// Path is $path — for the object form, $path.optional. nil when absent.
	Path *string
	// Children holds the non-"$"-prefixed members in JavaScript property
	// order (array-index-like keys first ascending, then insertion order).
	Children []TreeEntry
}

// TreeEntry is one named child of a Tree.
type TreeEntry struct {
	Name string
	Tree *Tree
}

type jsonKind int

const (
	// jsonInvalid models `undefined` after a failed JSON.parse: upstream's
	// try/finally still runs the validator over undefined, producing the
	// "Invalid configuration!" warning (RojoResolver.ts L229-236).
	jsonInvalid jsonKind = iota
	jsonNull
	jsonBool
	jsonNumber
	jsonString
	jsonArray
	jsonObject
)

type jsonValue struct {
	kind     jsonKind
	str      string
	num      float64
	members  []jsonMember // jsonObject, in JS property order
	elements []jsonValue  // jsonArray
}

type jsonMember struct {
	key   string
	value jsonValue
}

func (v jsonValue) member(key string) (jsonValue, bool) {
	for _, m := range v.members {
		if m.key == key {
			return m.value, true
		}
	}
	return jsonValue{}, false
}

// parseJSON parses data preserving object key order. Any syntax error yields
// jsonInvalid (JSON.parse would have thrown).
func parseJSON(data []byte) jsonValue {
	dec := json.NewDecoder(bytes.NewReader(data))
	dec.UseNumber()
	v, err := parseValue(dec)
	if err != nil {
		return jsonValue{kind: jsonInvalid}
	}
	if _, err := dec.Token(); err != io.EOF {
		return jsonValue{kind: jsonInvalid} // trailing garbage
	}
	return v
}

func parseValue(dec *json.Decoder) (jsonValue, error) {
	tok, err := dec.Token()
	if err != nil {
		return jsonValue{}, err
	}
	switch t := tok.(type) {
	case json.Delim:
		switch t {
		case '{':
			members := []jsonMember{}
			index := make(map[string]int)
			for dec.More() {
				keyTok, err := dec.Token()
				if err != nil {
					return jsonValue{}, err
				}
				key := keyTok.(string)
				val, err := parseValue(dec)
				if err != nil {
					return jsonValue{}, err
				}
				// Duplicate keys: later value wins, original position kept
				// (JS property assignment semantics).
				if i, ok := index[key]; ok {
					members[i].value = val
				} else {
					index[key] = len(members)
					members = append(members, jsonMember{key, val})
				}
			}
			if _, err := dec.Token(); err != nil { // consume '}'
				return jsonValue{}, err
			}
			return jsonValue{kind: jsonObject, members: orderLikeJS(members)}, nil
		default: // '['
			var elements []jsonValue
			for dec.More() {
				val, err := parseValue(dec)
				if err != nil {
					return jsonValue{}, err
				}
				elements = append(elements, val)
			}
			if _, err := dec.Token(); err != nil { // consume ']'
				return jsonValue{}, err
			}
			return jsonValue{kind: jsonArray, elements: elements}, nil
		}
	case string:
		return jsonValue{kind: jsonString, str: t}, nil
	case json.Number:
		f, err := t.Float64()
		if err != nil {
			return jsonValue{}, err
		}
		return jsonValue{kind: jsonNumber, num: f}, nil
	case bool:
		return jsonValue{kind: jsonBool}, nil
	default: // nil
		return jsonValue{kind: jsonNull}, nil
	}
}

// orderLikeJS reorders object members to JavaScript own-property order:
// array-index-like keys first in ascending numeric order, then the remaining
// string keys in insertion order. parseTree's Object.keys walk observes this
// order, which feeds partition LIFO ordering.
func orderLikeJS(members []jsonMember) []jsonMember {
	var indexKeys, stringKeys []jsonMember
	for _, m := range members {
		if isArrayIndexKey(m.key) {
			indexKeys = append(indexKeys, m)
		} else {
			stringKeys = append(stringKeys, m)
		}
	}
	if len(indexKeys) == 0 {
		return members
	}
	// Insertion sort keeps this dependency-free; index key counts are tiny.
	for i := 1; i < len(indexKeys); i++ {
		for j := i; j > 0; j-- {
			a, _ := strconv.ParseUint(indexKeys[j-1].key, 10, 64)
			b, _ := strconv.ParseUint(indexKeys[j].key, 10, 64)
			if a <= b {
				break
			}
			indexKeys[j-1], indexKeys[j] = indexKeys[j], indexKeys[j-1]
		}
	}
	return append(indexKeys, stringKeys...)
}

// isArrayIndexKey reports whether key is a canonical array index per
// ECMAScript (an integer 0..2^32-2 in canonical decimal form).
func isArrayIndexKey(key string) bool {
	n, err := strconv.ParseUint(key, 10, 64)
	if err != nil || n > 4294967294 {
		return false
	}
	return strconv.FormatUint(n, 10) == key
}

// validateConfig mirrors ajv validation of rojo-schema.json over the whole
// config document. It returns "" when valid, else a description shaped like
// ajv's errorsText ("data/tree must be object"). The exact wording is not
// byte-identical to ajv (warning text only — never reaches compiled output).
func validateConfig(v jsonValue) string {
	if v.kind != jsonObject {
		return "data must be object"
	}
	for _, required := range []string{"name", "tree"} {
		if _, ok := v.member(required); !ok {
			return "data must have required property '" + required + "'"
		}
	}
	for _, m := range v.members {
		switch m.key {
		case "name":
			if m.value.kind != jsonString {
				return "data/name must be string"
			}
		case "servePort":
			if m.value.kind != jsonNumber || math.Trunc(m.value.num) != m.value.num {
				return "data/servePort must be integer"
			}
		case "tree":
			if msg := validateTree(m.value, "data/tree"); msg != "" {
				return msg
			}
		}
	}
	return ""
}

func validateTree(v jsonValue, path string) string {
	if v.kind != jsonObject {
		return path + " must be object"
	}
	for _, m := range v.members {
		switch m.key {
		case "$className":
			if m.value.kind != jsonString {
				return path + "/$className must be string"
			}
		case "$ignoreUnknownInstances":
			if m.value.kind != jsonBool {
				return path + "/$ignoreUnknownInstances must be boolean"
			}
		case "$path":
			if !isValidPathValue(m.value) {
				return path + "/$path must match a schema in anyOf"
			}
		case "$properties":
			if m.value.kind != jsonObject {
				return path + "/$properties must be object"
			}
		default:
			// patternProperties ^[^\$].*$ — non-empty keys not starting
			// with "$" must be trees; other keys are unvalidated.
			if m.key != "" && !strings.HasPrefix(m.key, "$") {
				if msg := validateTree(m.value, path+"/"+m.key); msg != "" {
					return msg
				}
			}
		}
	}
	return ""
}

// isValidPathValue checks the $path anyOf: a string, or an object whose
// "optional" member (if present) is a string.
func isValidPathValue(v jsonValue) bool {
	if v.kind == jsonString {
		return true
	}
	if v.kind != jsonObject {
		return false
	}
	if opt, ok := v.member("optional"); ok && opt.kind != jsonString {
		return false
	}
	return true
}

// configFromJSON extracts (name, tree) from a VALIDATED config document.
func configFromJSON(v jsonValue) (string, *Tree) {
	name, _ := v.member("name")
	treeVal, _ := v.member("tree")
	return name.str, treeFromJSON(treeVal)
}

func treeFromJSON(v jsonValue) *Tree {
	tree := &Tree{}
	if v.kind != jsonObject {
		// Only reachable for ""-keyed children (which the schema does not
		// validate). Upstream would iterate Object.keys of the primitive;
		// we treat it as a childless tree instead.
		return tree
	}
	for _, m := range v.members {
		switch {
		case m.key == "$className":
			tree.ClassName = m.value.str
		case m.key == "$path":
			if m.value.kind == jsonString {
				s := m.value.str
				tree.Path = &s
			} else if opt, ok := m.value.member("optional"); ok {
				s := opt.str
				tree.Path = &s
			}
			// Object form without "optional" validates upstream but would
			// crash parseTree (path.resolve(basePath, undefined) throws);
			// we treat it as absent instead.
		case !strings.HasPrefix(m.key, "$"):
			tree.Children = append(tree.Children, TreeEntry{Name: m.key, Tree: treeFromJSON(m.value)})
		}
	}
	return tree
}

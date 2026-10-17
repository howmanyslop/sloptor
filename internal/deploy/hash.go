package deploy

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"os"
)

// CanonicalJSON marshals v deterministically: the value is round-tripped
// through encoding/json into generic maps/slices and re-marshaled, so object
// keys come out sorted (encoding/json sorts map[string]... keys) and numeric
// formatting is normalized regardless of the Go type that produced them.
func CanonicalJSON(v any) ([]byte, error) {
	data, err := json.Marshal(v)
	if err != nil {
		return nil, err
	}
	var norm any
	if err := json.Unmarshal(data, &norm); err != nil {
		return nil, err
	}
	return json.Marshal(norm)
}

// HashInputs returns the "sha256:<hex>" drift-detection hash of a resource's
// inputs, computed over their canonical JSON form.
func HashInputs(v any) (string, error) {
	data, err := CanonicalJSON(v)
	if err != nil {
		return "", fmt.Errorf("deploy: hashing inputs: %w", err)
	}
	sum := sha256.Sum256(data)
	return "sha256:" + hex.EncodeToString(sum[:]), nil
}

// HashFile returns the "sha256:<hex>" hash of a file's contents. Used to
// embed content hashes (place files, badge icons) in resource inputs so a
// rebuilt artifact shows up as an update in the plan.
func HashFile(path string) (string, error) {
	f, err := os.Open(path)
	if err != nil {
		return "", err
	}
	defer func() { _ = f.Close() }() // read-only; Close cannot lose data
	h := sha256.New()
	if _, err := io.Copy(h, f); err != nil {
		return "", err
	}
	return "sha256:" + hex.EncodeToString(h.Sum(nil)), nil
}

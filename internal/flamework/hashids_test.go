package flamework

import "testing"

func TestEncodeHashIDMatchesHashids228Oracle(t *testing.T) {
	t.Parallel()

	// Given: salts and numeric IDs characterized against hashids 2.2.8 in Node.
	tests := []struct {
		name string
		salt string
		id   uint64
		want string
	}{
		{name: "empty salt zero", salt: "", id: 0, want: "gY"},
		{name: "empty salt one", salt: "", id: 1, want: "jR"},
		{name: "salt one", salt: "salt", id: 1, want: "XG"},
		{name: "salt sixty two", salt: "salt", id: 62, want: "aaJ"},
		{name: "salt twelve thousand", salt: "salt", id: 12345, want: "X4j1"},
		{name: "maximum safe integer", salt: "salt", id: maxSafeHashID, want: "wQpRRqRX24R"},
		{name: "long salt", salt: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef", id: 12345, want: "aDgq"},
		{name: "unicode salt", salt: "🔥", id: 12345, want: "dN4G"},
		{name: "mixed unicode salt maximum", salt: "a🔥b", id: maxSafeHashID, want: "jka77A7QzW7"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			// When: the ID is encoded by the native implementation.
			got, err := EncodeHashID(tt.salt, tt.id)
			// Then: the exact upstream string is returned.
			if err != nil {
				t.Fatalf("EncodeHashID() error = %v", err)
			}
			if got != tt.want {
				t.Fatalf("EncodeHashID(%q, %d) = %q, want %q", tt.salt, tt.id, got, tt.want)
			}
		})
	}
}

func TestEncodeHashIDRejectsValuesOutsideJavaScriptSafeIntegerRange(t *testing.T) {
	t.Parallel()

	// Given: an integer Hashids 2.2.8 cannot represent safely as a JavaScript number.
	id := uint64(maxSafeHashID) + 1

	// When: the value is encoded.
	_, err := EncodeHashID("salt", id)

	// Then: native code rejects it instead of silently diverging from upstream.
	if err == nil {
		t.Fatalf("EncodeHashID(%d) error = nil, want range error", id)
	}
}

func TestEncodeHashIDIsStableAndCollisionFreeForFirstThousandIDs(t *testing.T) {
	t.Parallel()

	// Given: the first thousand incremental Flamework identifiers.
	seen := make(map[string]uint64, 1000)

	// When: every value is encoded twice with the same salt.
	for id := uint64(1); id <= 1000; id++ {
		first, err := EncodeHashID("salt", id)
		if err != nil {
			t.Fatalf("EncodeHashID(%d) error = %v", id, err)
		}
		second, err := EncodeHashID("salt", id)
		if err != nil {
			t.Fatalf("second EncodeHashID(%d) error = %v", id, err)
		}

		// Then: duplicates are stable and distinct inputs do not collide.
		if first != second {
			t.Fatalf("EncodeHashID(%d) changed from %q to %q", id, first, second)
		}
		if previous, exists := seen[first]; exists {
			t.Fatalf("IDs %d and %d both encoded as %q", previous, id, first)
		}
		seen[first] = id
	}

	if len(seen) != 1000 {
		t.Fatalf("encoded %d unique IDs, want 1000", len(seen))
	}
}

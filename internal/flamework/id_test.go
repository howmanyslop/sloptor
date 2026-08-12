package flamework

import (
	"bytes"
	"errors"
	"regexp"
	"testing"
)

func TestGenerateIdentifierMatchesUIDOracleForEveryMode(t *testing.T) {
	t.Parallel()

	// Given: one declaration and every upstream ID generation mode.
	base := IdentifierRequest{
		Salt:            "salt",
		HashPrefix:      "pkg",
		PackageName:     "@scope/pkg",
		InternalID:      "game:out/server/services/Foo@Bar",
		DeclarationName: "Bar",
		LuaFileName:     "foo",
		NextID:          1,
	}
	tests := []struct {
		mode IDMode
		want string
	}{
		{mode: IDModeFull, want: "pkg:server/services/Foo@Bar"},
		{mode: IDModeObfuscated, want: "pkg:XG"},
		{mode: IDModeShort, want: "pkg:foo@Bar{XG}"},
		{mode: IDModeTiny, want: "pkg:Bar{XG}"},
	}

	for _, tt := range tests {
		t.Run(string(tt.mode), func(t *testing.T) {
			t.Parallel()
			request := base
			request.Mode = tt.mode

			// When: the identifier is generated.
			got, err := GenerateIdentifier(request)
			// Then: it exactly matches uid.ts and Hashids 2.2.8.
			if err != nil {
				t.Fatalf("GenerateIdentifier() error = %v", err)
			}
			if got != tt.want {
				t.Fatalf("GenerateIdentifier() = %q, want %q", got, tt.want)
			}
		})
	}
}

func TestGenerateIdentifierMatchesGameAndPackageBranches(t *testing.T) {
	t.Parallel()

	// Given: upstream requests for an unprefixed game and external packages.
	tests := []struct {
		name    string
		request IdentifierRequest
		want    string
	}{
		{
			name: "game obfuscated ID has no package prefix",
			request: IdentifierRequest{
				Mode: IDModeObfuscated, Salt: "salt", PackageName: "game", IsGame: true, NextID: 1,
			},
			want: "XG",
		},
		{
			name: "package uses its build prefix",
			request: IdentifierRequest{
				Mode: IDModeFull, IsPackage: true, PackagePrefix: "@dep/pkg", InternalID: "@dep/pkg:out/shared/index@Thing",
			},
			want: "@dep/pkg:shared/index@Thing",
		},
		{
			name: "package without build prefix preserves internal ID",
			request: IdentifierRequest{
				Mode: IDModeFull, IsPackage: true, InternalID: "dep:src/index@Thing",
			},
			want: "dep:src/index@Thing",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			// When: the identifier is generated.
			got, err := GenerateIdentifier(tt.request)
			// Then: package/game prefix behavior matches uid.ts.
			if err != nil {
				t.Fatalf("GenerateIdentifier() error = %v", err)
			}
			if got != tt.want {
				t.Fatalf("GenerateIdentifier() = %q, want %q", got, tt.want)
			}
		})
	}
}

func TestGenerateIdentifierRejectsUnknownMode(t *testing.T) {
	t.Parallel()

	// Given: an ID mode outside the upstream union.
	request := IdentifierRequest{Mode: IDMode("future")}

	// When: generation is requested.
	_, err := GenerateIdentifier(request)

	// Then: the invalid mode is rejected.
	if !errors.Is(err, ErrInvalidIDMode) {
		t.Fatalf("GenerateIdentifier() error = %v, want ErrInvalidIDMode", err)
	}
}

func TestStableStringHashMemoizesByContextAndText(t *testing.T) {
	t.Parallel()

	// Given: deterministic UUID bytes and an initially empty persisted hash map.
	hashes := make(map[string]string)
	reader := bytes.NewReader([]byte{
		0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
		16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31,
	})

	// When: a duplicate and a different-context hash are requested.
	first, err := stableStringHash(hashes, "message", "remotes", reader)
	if err != nil {
		t.Fatalf("first stableStringHash() error = %v", err)
	}
	duplicate, err := stableStringHash(hashes, "message", "remotes", reader)
	if err != nil {
		t.Fatalf("duplicate stableStringHash() error = %v", err)
	}
	otherContext, err := stableStringHash(hashes, "message", DefaultHashContext, reader)
	if err != nil {
		t.Fatalf("other-context stableStringHash() error = %v", err)
	}

	// Then: the duplicate is stable and context is part of the persisted key.
	if first != "00010203-0405-4607-8809-0a0b0c0d0e0f" {
		t.Fatalf("first hash = %q", first)
	}
	if duplicate != first {
		t.Fatalf("duplicate hash = %q, want %q", duplicate, first)
	}
	if otherContext != "10111213-1415-4617-9819-1a1b1c1d1e1f" {
		t.Fatalf("other-context hash = %q", otherContext)
	}
	if hashes["remotes:message"] != first || hashes["@:message"] != otherContext {
		t.Fatalf("persisted hashes = %#v", hashes)
	}
}

func TestNewUUIDV4UsesRFC4122VersionAndVariantBits(t *testing.T) {
	t.Parallel()

	// Given: sixteen zero bytes from a cryptographic-reader seam.
	reader := bytes.NewReader(make([]byte, 16))

	// When: a UUID is generated.
	got, err := newUUIDV4(reader)
	// Then: version 4 and RFC 4122 variant bits are set.
	if err != nil {
		t.Fatalf("newUUIDV4() error = %v", err)
	}
	if got != "00000000-0000-4000-8000-000000000000" {
		t.Fatalf("newUUIDV4() = %q", got)
	}
}

func TestNewSaltUsesSixtyFourCryptographicBytes(t *testing.T) {
	t.Parallel()

	// Given: a deterministic 64-byte cryptographic-reader seam.
	reader := bytes.NewReader(make([]byte, 64))

	// When: a salt is generated.
	got, err := newSalt(reader)
	// Then: it is the same 128-character lowercase hex shape as crypto.randomBytes(64).
	if err != nil {
		t.Fatalf("newSalt() error = %v", err)
	}
	if matched := regexp.MustCompile(`^[0-9a-f]{128}$`).MatchString(got); !matched {
		t.Fatalf("newSalt() = %q, want 128 lowercase hex characters", got)
	}
}

func TestExportedCryptoPrimitivesUseFreshRandomnessAndPersistStringHashes(t *testing.T) {
	t.Parallel()

	// Given: the production cryptographic reader and an empty persisted hash map.
	hashes := make(map[string]string)

	// When: fresh UUIDs, a salt, and duplicate string hashes are requested.
	firstUUID, err := NewUUIDv4()
	if err != nil {
		t.Fatalf("first NewUUIDv4() error = %v", err)
	}
	secondUUID, err := NewUUIDv4()
	if err != nil {
		t.Fatalf("second NewUUIDv4() error = %v", err)
	}
	salt, err := NewSalt()
	if err != nil {
		t.Fatalf("NewSalt() error = %v", err)
	}
	firstHash, err := StableStringHash(hashes, "message", "remotes")
	if err != nil {
		t.Fatalf("first StableStringHash() error = %v", err)
	}
	duplicateHash, err := StableStringHash(hashes, "message", "remotes")
	if err != nil {
		t.Fatalf("duplicate StableStringHash() error = %v", err)
	}

	// Then: UUIDs are fresh v4 values, salts match randomBytes(64), and duplicate hashes are stable.
	uuidV4 := regexp.MustCompile(`^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$`)
	if !uuidV4.MatchString(firstUUID) || !uuidV4.MatchString(secondUUID) || !uuidV4.MatchString(firstHash) {
		t.Fatalf("UUID shapes = %q, %q, %q", firstUUID, secondUUID, firstHash)
	}
	if firstUUID == secondUUID {
		t.Fatalf("two fresh UUIDs both equal %q", firstUUID)
	}
	if !regexp.MustCompile(`^[0-9a-f]{128}$`).MatchString(salt) {
		t.Fatalf("NewSalt() = %q, want 128 lowercase hex characters", salt)
	}
	if duplicateHash != firstHash || hashes["remotes:message"] != firstHash {
		t.Fatalf("duplicate hash = %q, persisted = %#v, want %q", duplicateHash, hashes, firstHash)
	}
}

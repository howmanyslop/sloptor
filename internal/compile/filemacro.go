package compile

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"rotor/internal/transformer"
)

// fileResolver is the build-time data-file reader for the rotor $file macro
// rooted at the project directory. It
// resolves the macro argument project-relative, or
// relative to the importing source file when the path begins with `./` or `../`.
//
// Reads are bounded to a sane maximum so a stray huge file cannot blow up the
// compile; the $file macro is meant for small config/text data, not blobs.
type fileResolver struct {
	projectDir string // abs slash project dir
}

// maxFileMacroBytes caps a single $file read. Inlining megabytes of data into a
// Luau source would be pathological; a clear error beats an OOM.
const maxFileMacroBytes = 8 << 20 // 8 MiB

// newFileResolver builds the $file resolver for one compile pass.
func newFileResolver(projectDir string) *fileResolver {
	return &fileResolver{projectDir: filepath.ToSlash(projectDir)}
}

// Read implements transformer.FileResolver: returns the raw bytes of the file
// named by path (resolved relative to importerPath for `./`/`../`, else the
// project root). A missing/unreadable file is reported as
// transformer.ErrFileNotFound; an oversized file is a plain error.
func (r *fileResolver) Read(importerPath, path string) ([]byte, error) {
	absPath, err := r.resolvePath(importerPath, path)
	if err != nil {
		return nil, err
	}

	info, err := os.Stat(absPath)
	if err != nil || info.IsDir() {
		return nil, fmt.Errorf("%w: %s", transformer.ErrFileNotFound, path)
	}
	if info.Size() > maxFileMacroBytes {
		return nil, fmt.Errorf("$file %q is too large (%d bytes; limit %d)", path, info.Size(), maxFileMacroBytes)
	}

	data, err := os.ReadFile(absPath)
	if err != nil {
		return nil, fmt.Errorf("%w: %s", transformer.ErrFileNotFound, path)
	}
	return data, nil
}

// resolvePath turns the macro argument into an absolute filesystem path,
// relative to the project or the importing source.
func (r *fileResolver) resolvePath(importerPath, path string) (string, error) {
	p := strings.TrimSpace(path)
	if p == "" {
		return "", fmt.Errorf("%w: (empty path)", transformer.ErrFileNotFound)
	}
	slash := filepath.ToSlash(p)
	if strings.HasPrefix(slash, "./") || strings.HasPrefix(slash, "../") {
		base := r.projectDir
		if importerPath != "" {
			base = filepath.ToSlash(filepath.Dir(filepath.FromSlash(importerPath)))
		}
		return filepath.Join(filepath.FromSlash(base), filepath.FromSlash(slash)), nil
	}
	return filepath.Join(filepath.FromSlash(r.projectDir), filepath.FromSlash(slash)), nil
}

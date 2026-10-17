package compile

import (
	"bufio"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"rotor/internal/transformer"
)

// stampProviderOverride, when non-nil, replaces the real .git/time-backed
// provider in newProjectContext. It exists solely so tests can inject a stable
// fake (deterministic $git / $buildTime goldens); production never sets it.
var stampProviderOverride transformer.StampProvider

// gitStampProvider implements transformer.StampProvider by reading the project's
// .git directory natively where practical and shelling out only for the "dirty"
// check. It is constructed once per compile pass (newProjectContext) with a
// fixed build timestamp so every file in one build stamps the SAME $buildTime,
// and every git field is read at most once (memoized) so a multi-file build does
// no redundant work.
//
// Determinism contract (mirrored in the macro docs):
//   - GitSHA/GitBranch/GitTag/GitDirty are STABLE within a commit + working
//     tree, so $git-using files rebuild to identical bytes.
//   - BuildTime is the fixed per-pass timestamp; it is intentionally
//     non-deterministic across builds and busts caching for $buildTime files.
//
// Outside a git repo (no .git found walking up from the project dir): sha,
// branch, and tag are "" and dirty is false — never an error.
type gitStampProvider struct {
	gitDir    string // abs .git dir, or "" when not a repo
	workTree  string // abs working-tree root (parent of .git), or ""
	buildTime string // ISO-8601 timestamp, fixed for the whole pass

	once   sync.Once
	sha    string
	branch string
	tag    string

	dirtyOnce sync.Once
	dirty     bool
}

// resolveStampProvider returns the test override when set, else the real
// .git/time-backed provider for one compile pass.
func resolveStampProvider(projectDir string) transformer.StampProvider {
	if stampProviderOverride != nil {
		return stampProviderOverride
	}
	return newStampProvider(projectDir, time.Now())
}

// newStampProvider builds the $git / $buildTime provider for one compile pass.
// projectDir is the abs slash project dir; the .git directory is discovered by
// walking up from there. now is captured once so $buildTime is identical across
// every file in the build.
func newStampProvider(projectDir string, now time.Time) *gitStampProvider {
	gitDir, workTree := findGitDir(filepath.FromSlash(projectDir))
	return &gitStampProvider{
		gitDir:    gitDir,
		workTree:  workTree,
		buildTime: now.UTC().Format(time.RFC3339),
	}
}

// findGitDir walks up from dir looking for a `.git` directory (the common case)
// or a `.git` file (worktrees/submodules use `gitdir: <path>`). Returns the
// resolved git dir and the working-tree root, or "" / "" when none is found.
func findGitDir(dir string) (gitDir, workTree string) {
	cur := dir
	for {
		candidate := filepath.Join(cur, ".git")
		if info, err := os.Stat(candidate); err == nil {
			if info.IsDir() {
				return candidate, cur
			}
			// `.git` file: `gitdir: <path>` redirect (worktree/submodule).
			if data, err := os.ReadFile(candidate); err == nil {
				line := strings.TrimSpace(string(data))
				if rest, ok := strings.CutPrefix(line, "gitdir:"); ok {
					p := strings.TrimSpace(rest)
					if !filepath.IsAbs(p) {
						p = filepath.Join(cur, p)
					}
					return filepath.Clean(p), cur
				}
			}
			return candidate, cur
		}
		parent := filepath.Dir(cur)
		if parent == cur {
			return "", ""
		}
		cur = parent
	}
}

// loadGit resolves HEAD -> ref -> sha, the branch from HEAD, and the nearest tag
// pointing at that sha by scanning refs/tags + packed-refs. All native reads;
// failures degrade to "".
func (p *gitStampProvider) loadGit() {
	if p.gitDir == "" {
		return
	}
	headData, err := os.ReadFile(filepath.Join(p.gitDir, "HEAD"))
	if err != nil {
		return
	}
	head := strings.TrimSpace(string(headData))

	var fullSHA string
	if ref, ok := strings.CutPrefix(head, "ref:"); ok {
		ref = strings.TrimSpace(ref)
		// branch name is the last path segment of refs/heads/<name>.
		if name, ok := strings.CutPrefix(ref, "refs/heads/"); ok {
			p.branch = name
		}
		fullSHA = p.resolveRef(ref)
	} else {
		// Detached HEAD: the file holds the sha directly; no branch.
		fullSHA = head
	}

	if len(fullSHA) >= 7 {
		p.sha = fullSHA[:7]
	} else {
		p.sha = fullSHA
	}

	if fullSHA != "" {
		p.tag = p.nearestTag(fullSHA)
	}
}

// resolveRef returns the full sha a ref points at, checking the loose ref file
// first, then packed-refs. "" when unresolved.
func (p *gitStampProvider) resolveRef(ref string) string {
	if data, err := os.ReadFile(filepath.Join(p.gitDir, filepath.FromSlash(ref))); err == nil {
		return strings.TrimSpace(string(data))
	}
	// packed-refs: lines of "<sha> <refname>".
	if sha, ok := p.packedRefs()[ref]; ok {
		return sha
	}
	return ""
}

// nearestTag returns a tag name whose ref resolves to fullSHA (loose tags first,
// then packed-refs). Annotated tags whose object is the commit are matched via
// packed-refs' "^<sha>" peel lines. "" when no tag points at the commit.
func (p *gitStampProvider) nearestTag(fullSHA string) string {
	// Loose tags under refs/tags.
	tagsDir := filepath.Join(p.gitDir, "refs", "tags")
	if entries, err := os.ReadDir(tagsDir); err == nil {
		for _, entry := range entries {
			if entry.IsDir() {
				continue
			}
			if data, err := os.ReadFile(filepath.Join(tagsDir, entry.Name())); err == nil {
				if strings.TrimSpace(string(data)) == fullSHA {
					return entry.Name()
				}
			}
		}
	}
	// Packed tags: a "refs/tags/<name>" line whose sha matches, OR a peel line
	// "^<sha>" immediately following an annotated tag whose object is fullSHA.
	if f, err := os.Open(filepath.Join(p.gitDir, "packed-refs")); err == nil {
		defer func() { _ = f.Close() }() // read-only; nothing to flush
		scanner := bufio.NewScanner(f)
		lastTag := ""
		for scanner.Scan() {
			line := strings.TrimSpace(scanner.Text())
			if line == "" || strings.HasPrefix(line, "#") {
				continue
			}
			if peeled, ok := strings.CutPrefix(line, "^"); ok {
				// Peel line for the previous annotated tag.
				if lastTag != "" && peeled == fullSHA {
					return lastTag
				}
				continue
			}
			parts := strings.SplitN(line, " ", 2)
			if len(parts) != 2 {
				continue
			}
			sha, name := parts[0], parts[1]
			if tagName, ok := strings.CutPrefix(name, "refs/tags/"); ok {
				if sha == fullSHA {
					return tagName
				}
				lastTag = tagName
			} else {
				lastTag = ""
			}
		}
	}
	return ""
}

// packedRefs yields the refname->sha pairs from packed-refs (peel "^" lines
// skipped). Keyed by refname because multiple refs can share a sha (a branch and
// a tag at the same commit). Empty when the file is absent.
func (p *gitStampProvider) packedRefs() map[string]string {
	out := map[string]string{}
	f, err := os.Open(filepath.Join(p.gitDir, "packed-refs"))
	if err != nil {
		return out
	}
	defer func() { _ = f.Close() }() // read-only; nothing to flush
	scanner := bufio.NewScanner(f)
	for scanner.Scan() {
		line := strings.TrimSpace(scanner.Text())
		if line == "" || strings.HasPrefix(line, "#") || strings.HasPrefix(line, "^") {
			continue
		}
		parts := strings.SplitN(line, " ", 2)
		if len(parts) == 2 {
			out[parts[1]] = parts[0]
		}
	}
	return out
}

func (p *gitStampProvider) GitSHA() string    { p.once.Do(p.loadGit); return p.sha }
func (p *gitStampProvider) GitBranch() string { p.once.Do(p.loadGit); return p.branch }
func (p *gitStampProvider) GitTag() string    { p.once.Do(p.loadGit); return p.tag }

// GitDirty shells out to `git status --porcelain` (the reliable way to honor
// .gitignore and index state), memoized. Gracefully returns false when git is
// not on PATH or the project is not a repo.
func (p *gitStampProvider) GitDirty() bool {
	p.dirtyOnce.Do(func() {
		if p.workTree == "" {
			return
		}
		gitExe, err := exec.LookPath("git")
		if err != nil {
			return
		}
		cmd := exec.Command(gitExe, "status", "--porcelain")
		cmd.Dir = p.workTree
		out, err := cmd.Output()
		if err != nil {
			return
		}
		p.dirty = len(strings.TrimSpace(string(out))) > 0
	})
	return p.dirty
}

func (p *gitStampProvider) BuildTime() string { return p.buildTime }

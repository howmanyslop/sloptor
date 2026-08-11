package migrate

import (
	"crypto/sha256"
	"errors"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"

	"github.com/BurntSushi/toml"

	"rotor/internal/config"
)

func TestMergeFlameworkTOMLAppendsTableWithoutReserializingExistingBytes(t *testing.T) {
	path := filepath.Join(t.TempDir(), config.ConfigFileName)
	original := []byte("# keep this comment\r\n[assets]\r\nmode = \"macro\"\r\n")
	if err := os.WriteFile(path, original, 0o644); err != nil {
		t.Fatal(err)
	}

	change, status, err := MergeFlameworkTOML(path, FlameworkOptions{After: "prefix-transform"})
	if err != nil {
		t.Fatalf("MergeFlameworkTOML: %v", err)
	}
	if status != MergeReady {
		t.Fatalf("status = %v, want %v", status, MergeReady)
	}
	if !change.Existed || string(change.Original) != string(original) {
		t.Fatalf("change = %#v, want original bytes preserved", change)
	}
	if !strings.HasPrefix(string(change.Updated), string(original)) {
		t.Fatalf("updated TOML rewrote existing bytes:\n%s", change.Updated)
	}
	if strings.Contains(string(change.Updated), config.SchemaDirective) {
		t.Fatalf("existing TOML unexpectedly gained schema directive:\n%s", change.Updated)
	}
	if want := "[flamework]\nafter = \"prefix-transform\"\n"; !strings.HasSuffix(string(change.Updated), want) {
		t.Fatalf("updated TOML missing native table:\n%s", change.Updated)
	}
}

func TestMergeFlameworkTOMLCreatesSchemaOnlyForNewFile(t *testing.T) {
	path := filepath.Join(t.TempDir(), config.ConfigFileName)

	change, status, err := MergeFlameworkTOML(path, FlameworkOptions{})
	if err != nil {
		t.Fatalf("MergeFlameworkTOML: %v", err)
	}
	if status != MergeReady || change.Existed || len(change.Original) != 0 {
		t.Fatalf("change = %#v, status = %v", change, status)
	}
	want := config.SchemaDirective + "\n\n[flamework]\n"
	if string(change.Updated) != want {
		t.Fatalf("new TOML = %q, want %q", change.Updated, want)
	}
}

func TestMergeFlameworkTOMLRendersAllNativeOptionsAsValidTOML(t *testing.T) {
	limit := 7
	change, status, err := MergeFlameworkTOML(filepath.Join(t.TempDir(), config.ConfigFileName), FlameworkOptions{
		After: "prefix", NoSemanticDiagnostics: true, Obfuscation: true, IDGenerationMode: "tiny",
		HashPrefix: "game", Salt: "line\ncontrol\x01", PreloadIDs: true,
		Optimizations: FlameworkOptimizations{GuardGenerationDedupLimit: &limit},
	})
	if err != nil || status != MergeReady {
		t.Fatalf("MergeFlameworkTOML = (%v, %v), want ready", status, err)
	}
	for _, want := range []string{"after = \"prefix\"", "noSemanticDiagnostics = true", "obfuscation = true", "idGenerationMode = \"tiny\"", "hashPrefix = \"game\"", "salt = \"line\\ncontrol\\u0001\"", "preloadIds = true", "[flamework.optimizations]", "guardGenerationDedupLimit = 7"} {
		if !strings.Contains(string(change.Updated), want) {
			t.Fatalf("updated TOML missing %q:\n%s", want, change.Updated)
		}
	}
	var document map[string]any
	if _, err := toml.Decode(string(change.Updated), &document); err != nil {
		t.Fatalf("updated TOML does not parse: %v", err)
	}
}

func TestMergeFlameworkTOMLReturnsTypedStateWithoutMutating(t *testing.T) {
	tests := []struct {
		name    string
		content string
		status  MergeStatus
		kind    MergeErrorKind
	}{
		{"already migrated", "[flamework]\n", MergeAlreadyMigrated, MergeErrorAlreadyMigrated},
		{"invalid TOML", "[assets\n", MergeInvalid, MergeErrorInvalidTOML},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			path := filepath.Join(t.TempDir(), config.ConfigFileName)
			if err := os.WriteFile(path, []byte(test.content), 0o644); err != nil {
				t.Fatal(err)
			}

			_, status, err := MergeFlameworkTOML(path, FlameworkOptions{})
			var mergeErr *MergeError
			if !errors.As(err, &mergeErr) || mergeErr.Kind != test.kind {
				t.Fatalf("error = %v, want MergeError kind %v", err, test.kind)
			}
			if status != test.status {
				t.Fatalf("status = %v, want %v", status, test.status)
			}
			if got := readFile(t, path); got != test.content {
				t.Fatalf("preflight changed TOML: %q", got)
			}
		})
	}
}

func TestCommitCreatesUniqueBackupsOnlyForExistingChangedFiles(t *testing.T) {
	dir := t.TempDir()
	existing := filepath.Join(dir, "tsconfig.json")
	created := filepath.Join(dir, config.ConfigFileName)
	if err := os.WriteFile(existing, []byte("old tsconfig"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(existing+".bak", []byte("earlier backup"), 0o644); err != nil {
		t.Fatal(err)
	}

	receipt, err := Commit(dir, []FileChange{
		{Path: existing, Original: []byte("old tsconfig"), Updated: []byte("new tsconfig"), Existed: true},
		{Path: created, Updated: []byte("new rotor"), Existed: false},
	})
	if err != nil {
		t.Fatalf("Commit: %v", err)
	}
	if got := readFile(t, existing); got != "new tsconfig" {
		t.Fatalf("existing = %q", got)
	}
	if got := readFile(t, existing+".bak"); got != "earlier backup" {
		t.Fatalf("existing backup overwritten: %q", got)
	}
	if got := readFile(t, existing+".bak.1"); got != "old tsconfig" {
		t.Fatalf("new backup = %q", got)
	}
	if fileExists(created + ".bak") {
		t.Fatal("newly created rotor.toml has a backup")
	}
	if len(receipt.Backups) != 1 || receipt.Backups[0] != existing+".bak.1" {
		t.Fatalf("receipt backups = %v", receipt.Backups)
	}
}

func TestCommitRejectsIntermediateSymlinkEscapeWithoutMutatingExternalTree(t *testing.T) {
	project := t.TempDir()
	external := t.TempDir()
	link := filepath.Join(project, "linked")
	target := filepath.Join(link, "tsconfig.json")
	externalTarget := filepath.Join(external, "tsconfig.json")
	if err := os.WriteFile(externalTarget, []byte("outside original"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(external, link); err != nil {
		t.Fatal(err)
	}

	_, err := Commit(project, []FileChange{{
		Path: target, Original: []byte("outside original"), Updated: []byte("updated"), Existed: true,
	}})
	if !errors.Is(err, ErrTransactionConflict) {
		t.Fatalf("Commit error = %v, want transaction conflict", err)
	}
	if got := readFile(t, externalTarget); got != "outside original" {
		t.Fatalf("external target = %q, want unchanged", got)
	}
	if fileExists(externalTarget + ".bak") {
		t.Fatal("backup created outside project")
	}
	assertNoMigrationTemps(t, external)
	assertNoMigrationTemps(t, project)
}

func TestCommitRejectsChangesOutsideTrustedRootBeforeMutation(t *testing.T) {
	project := t.TempDir()
	external := filepath.Join(t.TempDir(), "outside.json")
	inside := filepath.Join(project, "inside.json")
	if err := os.WriteFile(inside, []byte("inside"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(external, []byte("outside"), 0o644); err != nil {
		t.Fatal(err)
	}

	_, err := Commit(project, []FileChange{
		{Path: inside, Original: []byte("inside"), Updated: []byte("changed"), Existed: true},
		{Path: external, Original: []byte("outside"), Updated: []byte("changed"), Existed: true},
	})
	if !errors.Is(err, ErrTransactionConflict) {
		t.Fatalf("Commit error = %v, want transaction conflict", err)
	}
	if got := readFile(t, inside); got != "inside" {
		t.Fatalf("inside target = %q, want unchanged", got)
	}
	if got := readFile(t, external); got != "outside" {
		t.Fatalf("outside target = %q, want unchanged", got)
	}
	assertNoMigrationTemps(t, project)
}

func TestCommitRejectsRelativeParentEscapeBeforeMutation(t *testing.T) {
	project := t.TempDir()
	target := filepath.Join(project, "inside.json")
	if err := os.WriteFile(target, []byte("inside"), 0o644); err != nil {
		t.Fatal(err)
	}

	_, err := Commit(project, []FileChange{{
		Path:     filepath.Join("..", filepath.Base(project), "inside.json"),
		Original: []byte("inside"), Updated: []byte("changed"), Existed: true,
	}})
	if !errors.Is(err, ErrTransactionConflict) {
		t.Fatalf("Commit error = %v, want transaction conflict", err)
	}
	if got := readFile(t, target); got != "inside" {
		t.Fatalf("target = %q, want unchanged", got)
	}
	assertNoMigrationTemps(t, project)
}

func TestCommitAcceptsTrustedRootSymlinkWhileConfiningChangesToItsReferent(t *testing.T) {
	container := t.TempDir()
	project := filepath.Join(container, "project")
	rootLink := filepath.Join(container, "project-link")
	if err := os.Mkdir(project, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(project, rootLink); err != nil {
		t.Fatal(err)
	}
	target := filepath.Join(rootLink, "tsconfig.json")
	if err := os.WriteFile(target, []byte("original"), 0o644); err != nil {
		t.Fatal(err)
	}

	_, err := Commit(rootLink, []FileChange{{
		Path: target, Original: []byte("original"), Updated: []byte("updated"), Existed: true,
	}})
	if err != nil {
		t.Fatalf("Commit: %v", err)
	}
	if got := readFile(t, filepath.Join(project, "tsconfig.json")); got != "updated" {
		t.Fatalf("target = %q, want updated", got)
	}
	if got := readFile(t, filepath.Join(project, "tsconfig.json.bak")); got != "original" {
		t.Fatalf("backup = %q, want original", got)
	}
}

func TestCommitRollbackUsesOpenedRootAfterTrustedRootIsMoved(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("Windows keeps an os.Root directory handle open, so the trusted root cannot be renamed")
	}
	container := t.TempDir()
	project := filepath.Join(container, "project")
	moved := filepath.Join(container, "project-moved")
	if err := os.Mkdir(project, 0o755); err != nil {
		t.Fatal(err)
	}
	first := filepath.Join(project, "first.json")
	second := filepath.Join(project, "second.json")
	for _, path := range []string{first, second} {
		if err := os.WriteFile(path, []byte("original"), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	tx := openTestTransactionFS(t, project)
	renames := 0
	_, err := tx.commitWithRename([]FileChange{
		{Path: first, Original: []byte("original"), Updated: []byte("updated first"), Existed: true},
		{Path: second, Original: []byte("original"), Updated: []byte("updated second"), Existed: true},
	}, func(oldName, newName string) error {
		renames++
		if renames != 2 {
			return tx.root.Rename(oldName, newName)
		}
		if err := os.Rename(project, moved); err != nil {
			return err
		}
		if err := os.Mkdir(project, 0o755); err != nil {
			return err
		}
		return errors.New("forced second rename failure")
	})
	var commitErr *CommitError
	if !errors.As(err, &commitErr) || !commitErr.RolledBack {
		t.Fatalf("Commit error = %v, want rolled-back CommitError", err)
	}
	for _, name := range []string{"first.json", "second.json"} {
		if got := readFile(t, filepath.Join(moved, name)); got != "original" {
			t.Fatalf("moved %s = %q, want original", name, got)
		}
	}
	entries, err := os.ReadDir(project)
	if err != nil {
		t.Fatal(err)
	}
	if len(entries) != 0 {
		t.Fatalf("replacement root mutated: %v", entries)
	}
	assertNoMigrationTemps(t, moved)
}

func TestCommitRollbackDoesNotFollowIntermediateSymlinkSwappedAfterRename(t *testing.T) {
	project := t.TempDir()
	external := t.TempDir()
	parent := filepath.Join(project, "nested")
	detached := filepath.Join(project, "nested-detached")
	if err := os.Mkdir(parent, 0o755); err != nil {
		t.Fatal(err)
	}
	first := filepath.Join(parent, "first.json")
	second := filepath.Join(parent, "second.json")
	for _, path := range []string{first, second} {
		if err := os.WriteFile(path, []byte("original"), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	externalSentinel := filepath.Join(external, "sentinel.json")
	if err := os.WriteFile(externalSentinel, []byte("outside"), 0o644); err != nil {
		t.Fatal(err)
	}
	tx := openTestTransactionFS(t, project)
	renames := 0
	_, err := tx.commitWithRename([]FileChange{
		{Path: first, Original: []byte("original"), Updated: []byte("updated first"), Existed: true},
		{Path: second, Original: []byte("original"), Updated: []byte("updated second"), Existed: true},
	}, func(oldName, newName string) error {
		renames++
		if err := tx.root.Rename(oldName, newName); err != nil {
			return err
		}
		if renames != 1 {
			return nil
		}
		if err := os.Rename(parent, detached); err != nil {
			return err
		}
		return os.Symlink(external, parent)
	})
	var commitErr *CommitError
	if !errors.As(err, &commitErr) || commitErr.RolledBack || !errors.Is(err, ErrTransactionConflict) {
		t.Fatalf("Commit error = %v, want rollback-conflict CommitError", err)
	}
	if got := readFile(t, externalSentinel); got != "outside" {
		t.Fatalf("external sentinel = %q, want unchanged", got)
	}
	externalEntries, err := os.ReadDir(external)
	if err != nil {
		t.Fatal(err)
	}
	if len(externalEntries) != 1 || externalEntries[0].Name() != "sentinel.json" {
		t.Fatalf("external tree mutated: %v", externalEntries)
	}
}

func TestCommitRejectsSymlinkToRegularTargetWithoutMutatingReferentOrLink(t *testing.T) {
	project := t.TempDir()
	referent := filepath.Join(t.TempDir(), "outside.json")
	target := filepath.Join(project, "tsconfig.json")
	if err := os.WriteFile(referent, []byte("original"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(referent, target); err != nil {
		t.Fatal(err)
	}
	backup := target + ".bak"
	if err := os.WriteFile(backup, []byte("previous backup"), 0o644); err != nil {
		t.Fatal(err)
	}
	referentHash := sha256.Sum256([]byte(readFile(t, referent)))
	targetHash := sha256.Sum256([]byte(readFile(t, target)))
	backupHash := sha256.Sum256([]byte(readFile(t, backup)))

	_, err := Commit(project, []FileChange{{
		Path: target, Original: []byte("original"), Updated: []byte("updated"), Existed: true,
	}})
	if !errors.Is(err, ErrTransactionConflict) {
		t.Errorf("Commit error = %v, want transaction conflict", err)
	}
	info, statErr := os.Lstat(target)
	if statErr != nil || info.Mode()&os.ModeSymlink == 0 {
		t.Errorf("target link = (%v, %v), want unchanged symlink", info, statErr)
	}
	if got := sha256.Sum256([]byte(readFile(t, referent))); got != referentHash {
		t.Errorf("referent hash = %x, want %x", got, referentHash)
	}
	if got := sha256.Sum256([]byte(readFile(t, target))); got != targetHash {
		t.Errorf("target hash = %x, want %x", got, targetHash)
	}
	if got := sha256.Sum256([]byte(readFile(t, backup))); got != backupHash {
		t.Errorf("backup hash = %x, want %x", got, backupHash)
	}
	if fileExists(backup + ".1") {
		t.Error("new backup created for rejected symlink")
	}
	assertNoMigrationTemps(t, project)
}

func TestCommitRejectsDanglingSymlinkTargetWithoutCreatingBackupOrTemp(t *testing.T) {
	project := t.TempDir()
	target := filepath.Join(project, "tsconfig.json")
	if err := os.Symlink(filepath.Join(project, "missing.json"), target); err != nil {
		t.Fatal(err)
	}

	_, err := Commit(project, []FileChange{{
		Path: target, Original: []byte("original"), Updated: []byte("updated"), Existed: true,
	}})
	if !errors.Is(err, ErrTransactionConflict) {
		t.Fatalf("Commit error = %v, want transaction conflict", err)
	}
	info, statErr := os.Lstat(target)
	if statErr != nil || info.Mode()&os.ModeSymlink == 0 {
		t.Fatalf("target link = (%v, %v), want unchanged dangling symlink", info, statErr)
	}
	if fileExists(target + ".bak") {
		t.Fatal("backup created for rejected dangling symlink")
	}
	assertNoMigrationTemps(t, project)
}

func TestCommitRejectsNonRegularTargetWithoutCreatingBackupOrTemp(t *testing.T) {
	project := t.TempDir()
	target := filepath.Join(project, "tsconfig.json")
	createNonRegularTarget(t, target)

	_, err := Commit(project, []FileChange{{
		Path: target, Original: []byte("original"), Updated: []byte("updated"), Existed: true,
	}})
	if !errors.Is(err, ErrTransactionConflict) {
		t.Fatalf("Commit error = %v, want transaction conflict", err)
	}
	info, statErr := os.Lstat(target)
	if statErr != nil || !isNonRegularTarget(info) {
		t.Fatalf("target mode = (%v, %v), want unchanged non-regular target", info, statErr)
	}
	if fileExists(target + ".bak") {
		t.Fatal("backup created for rejected FIFO")
	}
	assertNoMigrationTemps(t, project)
}

func TestCommitRejectsSymlinkBackupCollisionWithoutMutatingTargetOrReferent(t *testing.T) {
	project := t.TempDir()
	target := filepath.Join(project, "tsconfig.json")
	referent := filepath.Join(t.TempDir(), "outside-backup.json")
	if err := os.WriteFile(target, []byte("original"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(referent, []byte("outside backup"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(referent, target+".bak"); err != nil {
		t.Fatal(err)
	}
	targetHash := sha256.Sum256([]byte(readFile(t, target)))
	referentHash := sha256.Sum256([]byte(readFile(t, referent)))

	_, err := Commit(project, []FileChange{{
		Path: target, Original: []byte("original"), Updated: []byte("updated"), Existed: true,
	}})
	if !errors.Is(err, ErrTransactionConflict) {
		t.Fatalf("Commit error = %v, want transaction conflict", err)
	}
	if got := sha256.Sum256([]byte(readFile(t, target))); got != targetHash {
		t.Fatalf("target hash = %x, want %x", got, targetHash)
	}
	if got := sha256.Sum256([]byte(readFile(t, referent))); got != referentHash {
		t.Fatalf("referent hash = %x, want %x", got, referentHash)
	}
	info, statErr := os.Lstat(target + ".bak")
	if statErr != nil || info.Mode()&os.ModeSymlink == 0 {
		t.Fatalf("backup link = (%v, %v), want unchanged symlink", info, statErr)
	}
	if fileExists(target + ".bak.1") {
		t.Fatal("new backup created after rejected symlink collision")
	}
	assertNoMigrationTemps(t, project)
}

func TestValidateDestinationRejectsTargetLeafSwapBeforeAnyWrite(t *testing.T) {
	project := t.TempDir()
	target := filepath.Join(project, "tsconfig.json")
	referent := filepath.Join(t.TempDir(), "outside.json")
	if err := os.WriteFile(target, []byte("original"), 0o644); err != nil {
		t.Fatal(err)
	}
	tx := openTestTransactionFS(t, project)
	prepared, err := tx.preflight([]FileChange{{
		Path: target, Original: []byte("original"), Updated: []byte("updated"), Existed: true,
	}})
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(referent, []byte("outside"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.Remove(target); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(referent, target); err != nil {
		t.Fatal(err)
	}

	err = tx.validateDestination(prepared[0])
	if !errors.Is(err, ErrTransactionConflict) {
		t.Fatalf("validate destination error = %v, want transaction conflict", err)
	}
	info, statErr := os.Lstat(target)
	if statErr != nil || info.Mode()&os.ModeSymlink == 0 {
		t.Fatalf("target link = (%v, %v), want unchanged swapped symlink", info, statErr)
	}
	if fileExists(target + ".bak") {
		t.Fatal("backup created after target leaf swap")
	}
	assertNoMigrationTemps(t, project)
}

func TestValidateDestinationRejectsParentDirectorySwapBeforeAnyWrite(t *testing.T) {
	root := t.TempDir()
	parent := filepath.Join(root, "project")
	target := filepath.Join(parent, "tsconfig.json")
	if err := os.Mkdir(parent, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(target, []byte("original"), 0o644); err != nil {
		t.Fatal(err)
	}
	tx := openTestTransactionFS(t, root)
	prepared, err := tx.preflight([]FileChange{{
		Path: target, Original: []byte("original"), Updated: []byte("updated"), Existed: true,
	}})
	if err != nil {
		t.Fatal(err)
	}
	priorParent := filepath.Join(root, "project-before-swap")
	if err := os.Rename(parent, priorParent); err != nil {
		t.Fatal(err)
	}
	if err := os.Mkdir(parent, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(parent, "tsconfig.json"), []byte("replacement"), 0o644); err != nil {
		t.Fatal(err)
	}

	err = tx.validateDestination(prepared[0])
	if !errors.Is(err, ErrTransactionConflict) {
		t.Fatalf("validate destination error = %v, want transaction conflict", err)
	}
	if got := readFile(t, filepath.Join(priorParent, "tsconfig.json")); got != "original" {
		t.Fatalf("original parent target = %q, want original", got)
	}
	if got := readFile(t, filepath.Join(parent, "tsconfig.json")); got != "replacement" {
		t.Fatalf("swapped parent target = %q, want replacement", got)
	}
	if fileExists(target+".bak") || fileExists(filepath.Join(priorParent, "tsconfig.json.bak")) {
		t.Fatal("backup created after parent directory swap")
	}
	assertNoMigrationTemps(t, parent)
	assertNoMigrationTemps(t, priorParent)
}

func TestCommitRollsBackBeforeRejectingTargetLeafSwapBetweenRenames(t *testing.T) {
	project := t.TempDir()
	first := filepath.Join(project, "first.json")
	second := filepath.Join(project, "second.json")
	referent := filepath.Join(t.TempDir(), "outside.json")
	for _, path := range []string{first, second} {
		if err := os.WriteFile(path, []byte("original"), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	if err := os.WriteFile(referent, []byte("outside"), 0o644); err != nil {
		t.Fatal(err)
	}
	renames := 0
	tx := openTestTransactionFS(t, project)
	_, err := tx.commitWithRename([]FileChange{
		{Path: first, Original: []byte("original"), Updated: []byte("updated first"), Existed: true},
		{Path: second, Original: []byte("original"), Updated: []byte("updated second"), Existed: true},
	}, func(oldPath, newPath string) error {
		if renames == 0 {
			renames++
			if err := tx.root.Rename(oldPath, newPath); err != nil {
				return err
			}
			if err := os.Remove(second); err != nil {
				return err
			}
			return os.Symlink(referent, second)
		}
		renames++
		return tx.root.Rename(oldPath, newPath)
	})
	var commitErr *CommitError
	if !errors.As(err, &commitErr) || !commitErr.RolledBack || !errors.Is(err, ErrTransactionConflict) {
		t.Fatalf("Commit error = %v, want rolled-back transaction conflict", err)
	}
	if got := readFile(t, first); got != "original" {
		t.Fatalf("first after rollback = %q, want original", got)
	}
	info, statErr := os.Lstat(second)
	if statErr != nil || info.Mode()&os.ModeSymlink == 0 {
		t.Fatalf("second link = (%v, %v), want unchanged swapped symlink", info, statErr)
	}
	if got := readFile(t, referent); got != "outside" {
		t.Fatalf("referent = %q, want outside", got)
	}
	for _, path := range []string{first + ".bak", second + ".bak"} {
		if fileExists(path) {
			t.Fatalf("backup remains after rejected target swap: %s", path)
		}
	}
	assertNoMigrationTemps(t, project)
}

func TestCommitPreflightsEveryChangeBeforeMutation(t *testing.T) {
	dir := t.TempDir()
	first := filepath.Join(dir, "first.json")
	second := filepath.Join(dir, "second.json")
	for _, path := range []string{first, second} {
		if err := os.WriteFile(path, []byte("original"), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	if err := os.WriteFile(second, []byte("changed after planning"), 0o644); err != nil {
		t.Fatal(err)
	}

	_, err := Commit(dir, []FileChange{
		{Path: first, Original: []byte("original"), Updated: []byte("updated"), Existed: true},
		{Path: second, Original: []byte("original"), Updated: []byte("updated"), Existed: true},
	})
	if !errors.Is(err, ErrTransactionConflict) {
		t.Fatalf("Commit error = %v, want conflict", err)
	}
	if got := readFile(t, first); got != "original" {
		t.Fatalf("first changed before preflight completed: %q", got)
	}
	if fileExists(first + ".bak") {
		t.Fatal("backup created before preflight completed")
	}
}

func TestCommitRollsBackWhenSecondRenameFails(t *testing.T) {
	dir := t.TempDir()
	first := filepath.Join(dir, "first.json")
	second := filepath.Join(dir, "second.json")
	for _, path := range []string{first, second} {
		if err := os.WriteFile(path, []byte("original"), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	renames := 0
	tx := openTestTransactionFS(t, dir)
	_, err := tx.commitWithRename([]FileChange{
		{Path: first, Original: []byte("original"), Updated: []byte("updated one"), Existed: true},
		{Path: second, Original: []byte("original"), Updated: []byte("updated two"), Existed: true},
	}, func(oldPath, newPath string) error {
		renames++
		if renames == 2 {
			return errors.New("forced second rename failure")
		}
		return tx.root.Rename(oldPath, newPath)
	})
	var commitErr *CommitError
	if !errors.As(err, &commitErr) || !commitErr.RolledBack {
		t.Fatalf("error = %v, want rolled-back CommitError", err)
	}
	for _, path := range []string{first, second} {
		if got := readFile(t, path); got != "original" {
			t.Fatalf("%s after rollback = %q", filepath.Base(path), got)
		}
		if fileExists(path + ".bak") {
			t.Fatalf("%s backup remains after failed transaction", filepath.Base(path))
		}
	}
}

func readFile(t *testing.T, path string) string {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	return string(data)
}

func openTestTransactionFS(t *testing.T, root string) *transactionFS {
	t.Helper()
	tx, err := openTransactionFS(root)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() {
		if err := tx.root.Close(); err != nil {
			t.Errorf("close transaction root: %v", err)
		}
	})
	return tx
}

func fileExists(path string) bool {
	_, err := os.Stat(path)
	return err == nil
}

func assertNoMigrationTemps(t *testing.T, dir string) {
	t.Helper()
	temps, err := filepath.Glob(filepath.Join(dir, ".rotor-migrate-*"))
	if err != nil {
		t.Fatal(err)
	}
	if len(temps) != 0 {
		t.Fatalf("migration temps remain: %v", temps)
	}
}

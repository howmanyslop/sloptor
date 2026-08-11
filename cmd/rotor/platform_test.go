package main

import (
	"os"
	"path/filepath"
	"runtime"
	"testing"
)

func writePlatformShim(t *testing.T, directory, name, unixBody, windowsBody string) string {
	t.Helper()
	if runtime.GOOS == "windows" {
		path := filepath.Join(directory, name+".cmd")
		writeTestFile(t, path, "@echo off\r\n"+windowsBody)
		return path
	}
	path := filepath.Join(directory, name)
	writeTestFile(t, path, unixBody)
	if err := os.Chmod(path, 0o755); err != nil {
		t.Fatal(err)
	}
	return path
}

func platformExecutablePath(path string) string {
	if runtime.GOOS == "windows" && filepath.Ext(path) == "" {
		return path + ".exe"
	}
	return path
}

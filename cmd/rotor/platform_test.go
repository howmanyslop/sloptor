package main

import (
	"path/filepath"
	"runtime"
)

func platformExecutablePath(path string) string {
	if runtime.GOOS == "windows" && filepath.Ext(path) == "" {
		return path + ".exe"
	}
	return path
}

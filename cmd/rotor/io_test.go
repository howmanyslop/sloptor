package main

import (
	"io"
	"os"
	"testing"
)

func captureStderr(t *testing.T, fn func() int) (string, int) {
	t.Helper()
	prev := os.Stderr
	r, w, err := os.Pipe()
	if err != nil {
		t.Fatal(err)
	}
	os.Stderr = w
	done := make(chan struct{})
	var data []byte
	go func() {
		data, _ = io.ReadAll(r)
		close(done)
	}()
	code := fn()
	_ = w.Close()
	os.Stderr = prev
	<-done
	return string(data), code
}

func fileExists(path string) bool {
	st, err := os.Stat(path)
	return err == nil && st.Mode().IsRegular()
}

func dirExists(path string) bool {
	st, err := os.Stat(path)
	return err == nil && st.IsDir()
}

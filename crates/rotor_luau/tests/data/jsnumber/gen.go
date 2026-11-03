//go:build ignore

// Generates the js_number_string oracle fixtures from Go's JSNumberString.
// Run from the repository root:
//
//	go run ./crates/rotor_luau/tests/data/jsnumber/gen.go
//
// edges.txt holds "<hex bits> <expected>" lines for hand-picked doubles.
// random.txt holds one expected string per line for the bit patterns drawn
// from splitmix64 seeded with 1337; the Rust test regenerates the same bits.
package main

import (
	"bufio"
	"fmt"
	"math"
	"os"
	"path/filepath"
	"runtime"

	"rotor/internal/luau"
)

const randomCount = 200_000

func splitmix(state *uint64) uint64 {
	*state += 0x9E3779B97F4A7C15
	z := *state
	z = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9
	z = (z ^ (z >> 27)) * 0x94D049BB133111EB
	return z ^ (z >> 31)
}

func edges() []float64 {
	values := []float64{
		0, math.Copysign(0, -1),
		math.SmallestNonzeroFloat64, math.Float64frombits(0x000fffffffffffff), math.Float64frombits(0x0010000000000000),
		math.MaxFloat64, -math.MaxFloat64, -math.SmallestNonzeroFloat64,
		math.NaN(), math.Inf(1), math.Inf(-1),
		1, -1, 0.1, 0.5, 1.5, 2.5, 123.456,
		1 << 53, 1<<53 - 1, 1<<53 + 2, -(1 << 53), 1 << 63, 1 << 64, math.MaxInt32, math.MinInt32, math.MaxUint32,
		1e20, 1e21, 1e-6, 1e-7, 5e-324,
		75751872977403.125, 562949953421312.25, math.Float64frombits(4827957688934510274),
	}
	for _, boundary := range []float64{1 << 53, 1e21, 1e-6, 1e-7, 1e15, 1e16, 1e17} {
		values = append(values, math.Nextafter(boundary, 0), math.Nextafter(boundary, math.Inf(1)))
	}
	return values
}

func write(path string, lines func(*bufio.Writer)) {
	file, err := os.Create(path)
	if err != nil {
		panic(err)
	}
	defer file.Close()
	writer := bufio.NewWriter(file)
	lines(writer)
	if err := writer.Flush(); err != nil {
		panic(err)
	}
}

func main() {
	_, self, _, _ := runtime.Caller(0)
	dir := filepath.Dir(self)
	write(filepath.Join(dir, "edges.txt"), func(w *bufio.Writer) {
		for _, value := range edges() {
			fmt.Fprintf(w, "%016x %s\n", math.Float64bits(value), luau.JSNumberString(value))
		}
	})
	write(filepath.Join(dir, "random.txt"), func(w *bufio.Writer) {
		state := uint64(1337)
		for range randomCount {
			fmt.Fprintln(w, luau.JSNumberString(math.Float64frombits(splitmix(&state))))
		}
	})
}

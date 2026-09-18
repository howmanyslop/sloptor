//go:build !darwin && !linux && !windows

package compile

func sidecarProcessGeneration(int) string {
	return ""
}

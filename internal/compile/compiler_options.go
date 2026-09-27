package compile

import "rotor/tsgo/core"

// ApplyAutomaticTypes restores TypeScript 5's automatic typeRoots inclusion
// after the full tsconfig extends chain has been resolved. TypeScript 7 only
// performs that walk when Types contains the "*" compatibility marker.
//
// A non-nil slice, including an explicitly empty one, came from the resolved
// config and must remain untouched.
func ApplyAutomaticTypes(options *core.CompilerOptions) {
	if options.Types == nil {
		options.Types = []string{"*"}
	}
}

func ApplyCheckerOverride(options *core.CompilerOptions, checkers *int) {
	if checkers != nil {
		options.Checkers = checkers
	}
}

func ApplySingleThreadedOverride(options *core.CompilerOptions, singleThreaded *bool) {
	if singleThreaded == nil {
		return
	}
	if *singleThreaded {
		options.SingleThreaded = core.TSTrue
		return
	}
	options.SingleThreaded = core.TSFalse
}

func applyCheckerOverride(options *core.CompilerOptions, checkers *int) {
	ApplyCheckerOverride(options, checkers)
}

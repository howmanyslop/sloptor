package compile

import "rotor/tsgo/core"

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

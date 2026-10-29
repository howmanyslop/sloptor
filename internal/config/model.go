package config

type Config struct {
	ParallelTransformers *ParallelTransformersConfig `json:"parallelTransformers,omitempty" toml:"parallelTransformers,omitempty"`
	Flamework            *FlameworkConfig            `json:"flamework,omitempty" toml:"flamework,omitempty"`

	// NativeTypes is the rotor.toml half of the `//!native` type-annotation
	// opt-in: when true, a file whose leading comments include `//!native`
	// emits Luau parameter and return annotations. It is a pointer so an unset
	// value is distinguishable from an explicit false — tsconfig.json's
	// `rbxts.nativeTypes` wins when it is set, and this value is the fallback
	// only. Both unset means off.
	NativeTypes *bool `json:"nativeTypes,omitempty" toml:"nativeTypes,omitempty"`

	// LuauTypes is the rotor.toml half of the experimental `luauTypes`
	// opt-in: when true, compiled output carries full Luau type annotations
	// (parameters, returns, locals, class fields, exported type aliases) so
	// Luau code can consume it with real types. It is a pointer so an unset
	// value is distinguishable from an explicit false — `--luauTypes` on the
	// command line and tsconfig.json's `rbxts.luauTypes` win when they are
	// set, and this value is the fallback only. Unset everywhere means off.
	// The emitted output may change between releases.
	LuauTypes *bool `json:"luauTypes,omitempty" toml:"luauTypes,omitempty"`

	Warnings []string `json:"-" toml:"-"`
}

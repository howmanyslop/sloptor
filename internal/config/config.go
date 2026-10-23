// Package config loads native compiler settings from rotor.toml.
package config

// Validate returns structural validation errors for compiler settings.
func (c *Config) Validate() []error {
	return c.ValidateFlamework()
}

// Package cloud is a Roblox Open Cloud REST client: the shared foundation
// for rotor's asset-sync and deploy tools. It owns auth (x-api-key), per-host
// token-bucket rate limiting, retries with exponential backoff + jitter on
// 429/5xx (honoring Retry-After), long-running-operation polling, and typed
// wrappers for the endpoints assets/deploy need (assets, universes, places,
// place publishing, badges, game passes, developer products, social links,
// experience icon/thumbnails).
//
// Everything is context-aware and the base URL is an option, so tests run
// against httptest servers; no network is touched in unit tests.
package cloud

import (
	"errors"
	"net/http"
	"os"
	"time"
)

// defaultBaseURL is the production Open Cloud host. All endpoint paths in
// this package are relative to it.
const defaultBaseURL = "https://apis.roblox.com"

// ErrNoAPIKey is returned by FromEnv when ROBLOX_API_KEY is unset or empty.
// Callers (cmd/rotor) wrap it with the Creator Dashboard URL and the scopes
// the failing command needs.
var ErrNoAPIKey = errors.New("cloud: ROBLOX_API_KEY is not set")

// Client is a Roblox Open Cloud client. Construct with New or FromEnv; the
// zero value is not usable.
type Client struct {
	apiKey     string
	baseURL    string
	userAgent  string
	httpClient *http.Client
	limiter    *limiter

	// Retry/poll tuning. Unexported so production behavior stays uniform;
	// tests shrink these to keep retry/backoff paths fast.
	maxAttempts int           // total tries per request, including the first
	retryBase   time.Duration // first backoff step (doubles per attempt)
	retryCap    time.Duration // ceiling for a single backoff sleep
	pollBase    time.Duration // first operation-poll interval
	pollCap     time.Duration // ceiling for the operation-poll interval
}

// Option configures a Client at construction time.
type Option func(*Client)

// WithBaseURL overrides the API host (e.g. an httptest server URL). A
// trailing slash is tolerated.
func WithBaseURL(u string) Option {
	return func(c *Client) {
		for len(u) > 0 && u[len(u)-1] == '/' {
			u = u[:len(u)-1]
		}
		c.baseURL = u
	}
}

// WithUserAgent overrides the User-Agent header sent on every request.
func WithUserAgent(ua string) Option {
	return func(c *Client) { c.userAgent = ua }
}

// WithRateLimit reconfigures the per-host token bucket. perSecond <= 0
// disables client-side rate limiting entirely (server 429s still back off).
func WithRateLimit(perSecond float64, burst int) Option {
	return func(c *Client) { c.limiter = newLimiter(perSecond, burst) }
}

// New returns a Client authenticating with apiKey. The default configuration
// targets production: https://apis.roblox.com, a modest 5 req/s (burst 10)
// per-host rate limit, and up to 5 attempts per request.
func New(apiKey string, opts ...Option) *Client {
	c := &Client{
		apiKey:      apiKey,
		baseURL:     defaultBaseURL,
		userAgent:   "rotor",
		httpClient:  &http.Client{Timeout: 5 * time.Minute},
		limiter:     newLimiter(5, 10),
		maxAttempts: 5,
		retryBase:   500 * time.Millisecond,
		retryCap:    30 * time.Second,
		pollBase:    500 * time.Millisecond,
		pollCap:     8 * time.Second,
	}
	for _, opt := range opts {
		opt(c)
	}
	return c
}

// FromEnv builds a Client from the ROBLOX_API_KEY environment variable,
// returning ErrNoAPIKey when it is missing — rotor never reads keys from
// disk.
func FromEnv(opts ...Option) (*Client, error) {
	key := os.Getenv("ROBLOX_API_KEY")
	if key == "" {
		return nil, ErrNoAPIKey
	}
	return New(key, opts...), nil
}

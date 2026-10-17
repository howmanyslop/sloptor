package cloud

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"math/rand/v2"
	"net/http"
	"net/url"
	"strconv"
	"time"
)

// APIError is a non-2xx Open Cloud response, surfaced after retries are
// exhausted (or immediately for non-retryable statuses). Code/Message come
// from the error JSON body when one is present.
type APIError struct {
	StatusCode int
	Code       string
	Message    string

	// retryAfter carries the server's Retry-After hint between retry
	// iterations; unexported, transport-internal.
	retryAfter time.Duration
}

func (e *APIError) Error() string {
	msg := e.Message
	if msg == "" {
		msg = http.StatusText(e.StatusCode)
	}
	if e.Code != "" {
		return fmt.Sprintf("cloud: HTTP %d (%s): %s", e.StatusCode, e.Code, msg)
	}
	return fmt.Sprintf("cloud: HTTP %d: %s", e.StatusCode, msg)
}

// parseAPIError extracts code/message from the several error JSON shapes
// Open Cloud uses: {"code","message"} (cloud v2 / assets), {"error","message"}
// (some v1 routes), and {"errors":[{"code","message"}]} (legacy endpoints,
// numeric codes).
func parseAPIError(status int, body []byte) *APIError {
	e := &APIError{StatusCode: status}
	var shape struct {
		Code    json.RawMessage `json:"code"`
		Message string          `json:"message"`
		Error   json.RawMessage `json:"error"`
		Errors  []struct {
			Code    json.RawMessage `json:"code"`
			Message string          `json:"message"`
		} `json:"errors"`
	}
	if json.Unmarshal(body, &shape) != nil {
		return e
	}
	e.Message = shape.Message
	e.Code = rawToString(shape.Code)
	if e.Code == "" {
		e.Code = rawToString(shape.Error)
	}
	if e.Code == "" && e.Message == "" && len(shape.Errors) > 0 {
		e.Code = rawToString(shape.Errors[0].Code)
		e.Message = shape.Errors[0].Message
	}
	return e
}

// rawToString renders a JSON scalar (string or number) as a plain string.
func rawToString(raw json.RawMessage) string {
	if len(raw) == 0 {
		return ""
	}
	var s string
	if json.Unmarshal(raw, &s) == nil {
		return s
	}
	return string(bytes.TrimSpace(raw))
}

// do issues one logical request: rate-limit wait, then up to c.maxAttempts
// tries with exponential backoff + jitter on 429/5xx/transport errors,
// honoring Retry-After. Bodies are []byte (not io.Reader) precisely so every
// retry can resend identical bytes. On 2xx the response body is decoded into
// out when out is non-nil.
func (c *Client) do(ctx context.Context, method, path string, query url.Values, contentType string, body []byte, out any) error {
	u := c.baseURL + path
	if len(query) > 0 {
		u += "?" + query.Encode()
	}
	host := hostOf(u)

	var lastErr error
	for attempt := 0; attempt < c.maxAttempts; attempt++ {
		if attempt > 0 {
			if err := sleep(ctx, c.backoff(attempt-1, lastErr)); err != nil {
				return err
			}
		}
		if err := c.limiter.wait(ctx, host); err != nil {
			return err
		}

		var reader io.Reader
		if body != nil {
			reader = bytes.NewReader(body)
		}
		req, err := http.NewRequestWithContext(ctx, method, u, reader)
		if err != nil {
			return err
		}
		req.Header.Set("x-api-key", c.apiKey)
		req.Header.Set("User-Agent", c.userAgent)
		if contentType != "" {
			req.Header.Set("Content-Type", contentType)
		}

		resp, err := c.httpClient.Do(req)
		if err != nil {
			// Transport errors (conn reset, etc.) retry like 5xx, but a dead
			// context ends the loop immediately.
			if ctx.Err() != nil {
				return ctx.Err()
			}
			lastErr = err
			continue
		}
		respBody, readErr := io.ReadAll(resp.Body)
		_ = resp.Body.Close() // body fully read; readErr carries any real failure
		if readErr != nil {
			lastErr = readErr
			continue
		}

		switch {
		case resp.StatusCode >= 200 && resp.StatusCode < 300:
			if out != nil && len(respBody) > 0 {
				if err := json.Unmarshal(respBody, out); err != nil {
					return fmt.Errorf("cloud: decoding %s %s response: %w", method, path, err)
				}
			}
			return nil
		case resp.StatusCode == http.StatusTooManyRequests || resp.StatusCode >= 500:
			apiErr := parseAPIError(resp.StatusCode, respBody)
			apiErr.retryAfter = retryAfter(resp.Header)
			lastErr = apiErr
			continue
		default:
			return parseAPIError(resp.StatusCode, respBody)
		}
	}
	return lastErr
}

// backoff picks the sleep before retry number n (0-based): Retry-After when
// the server sent one, otherwise retryBase·2ⁿ capped at retryCap, jittered
// into [d/2, 3d/2) so concurrent clients don't stampede in lockstep.
func (c *Client) backoff(n int, lastErr error) time.Duration {
	if apiErr, ok := lastErr.(*APIError); ok && apiErr.retryAfter > 0 {
		return apiErr.retryAfter
	}
	d := c.retryBase << n
	if d > c.retryCap {
		d = c.retryCap
	}
	if d <= 0 {
		return 0
	}
	return d/2 + rand.N(d)
}

// retryAfter parses a Retry-After header: either delta-seconds or an HTTP
// date. Returns 0 when absent or unparseable.
func retryAfter(h http.Header) time.Duration {
	v := h.Get("Retry-After")
	if v == "" {
		return 0
	}
	if secs, err := strconv.Atoi(v); err == nil {
		if secs < 0 {
			return 0
		}
		return time.Duration(secs) * time.Second
	}
	if t, err := http.ParseTime(v); err == nil {
		if d := time.Until(t); d > 0 {
			return d
		}
	}
	return 0
}

// doJSON marshals in (when non-nil) and dispatches with an application/json
// content type.
func (c *Client) doJSON(ctx context.Context, method, path string, query url.Values, in, out any) error {
	var body []byte
	contentType := ""
	if in != nil {
		var err error
		body, err = json.Marshal(in)
		if err != nil {
			return err
		}
		contentType = "application/json"
	}
	return c.do(ctx, method, path, query, contentType, body, out)
}

// hostOf extracts the host of a URL for rate-limit bucketing; the raw string
// is the fallback key when parsing fails (still a stable bucket).
func hostOf(u string) string {
	parsed, err := url.Parse(u)
	if err != nil || parsed.Host == "" {
		return u
	}
	return parsed.Host
}

// sleep blocks for d or until ctx is done, whichever comes first.
func sleep(ctx context.Context, d time.Duration) error {
	if d <= 0 {
		return ctx.Err()
	}
	t := time.NewTimer(d)
	defer t.Stop()
	select {
	case <-ctx.Done():
		return ctx.Err()
	case <-t.C:
		return nil
	}
}

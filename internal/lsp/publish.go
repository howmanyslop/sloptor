package lsp

import (
	"os"
	"slices"
	"strings"

	"rotor/internal/compile"
	"rotor/tsgo/core"
	"rotor/tsgo/ls/lsconv"
	"rotor/tsgo/lsp/lsproto"
)

// internalErrorCode is the code given to the diagnostic that stands in for a
// transform that panicked; the census carries no code of its own for it.
const internalErrorCode = "internalCompilerError"

// scriptText adapts a file's name and text to lsconv's Script.
type scriptText struct {
	name string
	text string
}

func (s scriptText) FileName() string { return s.name }
func (s scriptText) Text() string     { return s.text }

// isTypeScriptCode reports whether code is a TypeScript diagnostic ("TS####"),
// as opposed to one of sloptor's own factory names.
func isTypeScriptCode(code string) bool {
	return strings.HasPrefix(code, "TS")
}

// publish turns one census into protocol traffic: per-file diagnostics, a clear
// for every file that has gone clean, and window messages for what cannot be
// pinned to a file.
//
// overlays is the same snapshot the census compiled, so an offset is always
// mapped through the text it was measured against, even if the client has
// edited the buffer since.
func (s *server) publish(config string, overlays map[string]string, result *compile.ProjectDiagnostics, censusErr error) {
	if result != nil && (censusErr == nil || len(result.Files) > 0) {
		s.publishFiles(config, overlays, result.Files)
	}
	s.showMessages(config, result, censusErr)
}

func (s *server) publishFiles(config string, overlays map[string]string, files []compile.FileDiagnostics) {
	// Program order keeps the output deterministic: the same census always
	// publishes in the same sequence.
	type entry struct {
		uri         lsproto.DocumentUri
		diagnostics []*lsproto.Diagnostic
	}
	var next []entry
	current := make(map[lsproto.DocumentUri]struct{})
	for i := range files {
		file := &files[i]
		diagnostics := s.fileDiagnostics(file, overlays)
		if len(diagnostics) == 0 {
			continue
		}
		uri := lsconv.FileNameToDocumentURI(file.FileName)
		next = append(next, entry{uri, diagnostics})
		current[uri] = struct{}{}
	}

	s.mu.Lock()
	previous := s.published[config]
	s.published[config] = current
	s.mu.Unlock()

	for _, e := range next {
		s.publishDiagnostics(e.uri, e.diagnostics)
	}
	var cleared []lsproto.DocumentUri
	for uri := range previous {
		if _, still := current[uri]; !still {
			cleared = append(cleared, uri)
		}
	}
	slices.Sort(cleared)
	for _, uri := range cleared {
		s.publishDiagnostics(uri, []*lsproto.Diagnostic{})
	}
}

func (s *server) publishDiagnostics(uri lsproto.DocumentUri, diagnostics []*lsproto.Diagnostic) {
	s.notify(lsproto.TextDocumentPublishDiagnosticsInfo.NewNotificationMessage(&lsproto.PublishDiagnosticsParams{
		Uri:         uri,
		Diagnostics: diagnostics,
	}))
}

// fileDiagnostics converts one census entry. The text offsets refer to is the
// open buffer when there is one, and the file on disk otherwise.
func (s *server) fileDiagnostics(file *compile.FileDiagnostics, overlays map[string]string) []*lsproto.Diagnostic {
	if file.InternalError != nil {
		// The panic leaves no trustworthy span, and the file's other
		// diagnostics were produced by a transform that did not finish.
		return []*lsproto.Diagnostic{newDiagnostic(
			lsproto.Range{},
			internalErrorCode,
			file.InternalError.Error(),
			lsproto.DiagnosticSeverityError,
		)}
	}

	var kept []*compile.DiagnosticInfo
	for i := range file.Diagnostics {
		info := &file.Diagnostics[i]
		if isTypeScriptCode(info.Code) && !s.typeErrors {
			continue
		}
		kept = append(kept, info)
	}
	if len(kept) == 0 {
		return nil
	}

	script := scriptText{name: file.FileName, text: sourceText(file.FileName, overlays)}
	lineMap := lsconv.ComputeLSPLineStarts(script.text)
	converters := lsconv.NewConverters(s.encoding, func(string) *lsconv.LSPLineMap { return lineMap })

	diagnostics := make([]*lsproto.Diagnostic, 0, len(kept))
	for _, info := range kept {
		severity := lsproto.DiagnosticSeverityError
		if info.Warning {
			severity = lsproto.DiagnosticSeverityWarning
		}
		span := core.NewTextRange(info.Offset, info.Offset+info.Len)
		diagnostics = append(diagnostics, newDiagnostic(converters.ToLSPRange(script, span), info.Code, info.Message, severity))
	}
	return diagnostics
}

// sourceText is the text a file's offsets were measured against: its open
// buffer if the editor has one, else what is on disk. A file that cannot be
// read yields empty text, which clamps every position to the start rather than
// dropping the diagnostic.
func sourceText(fileName string, overlays map[string]string) string {
	path := sourcePathKey(fileName)
	if text, ok := overlays[path]; ok {
		return text
	}
	data, err := os.ReadFile(path)
	if err != nil {
		return ""
	}
	return string(data)
}

func newDiagnostic(rng lsproto.Range, code, message string, severity lsproto.DiagnosticSeverity) *lsproto.Diagnostic {
	source := serverName
	diagnostic := &lsproto.Diagnostic{
		Range:    rng,
		Severity: &severity,
		Source:   &source,
		Message:  lsproto.StringOrMarkupContent{String: &message},
	}
	if code != "" {
		diagnostic.Code = &lsproto.IntegerOrString{String: &code}
	}
	return diagnostic
}

// showMessages reports failures that belong to no file as window messages: the
// census error itself and the project-level diagnostics. A problem usually
// outlives many censuses, so a message is sent once and only again if it went
// away and came back. Otherwise every keystroke would raise the same popup.
func (s *server) showMessages(config string, result *compile.ProjectDiagnostics, censusErr error) {
	type message struct {
		text string
		kind lsproto.MessageType
	}
	var messages []message
	if censusErr != nil {
		messages = append(messages, message{censusErr.Error(), lsproto.MessageTypeError})
	}
	if result != nil {
		for i := range result.Diagnostics {
			info := &result.Diagnostics[i]
			if isTypeScriptCode(info.Code) && !s.typeErrors {
				continue
			}
			kind := lsproto.MessageTypeError
			if info.Warning {
				kind = lsproto.MessageTypeWarning
			}
			messages = append(messages, message{info.Message, kind})
		}
	}

	current := make(map[string]struct{}, len(messages))
	var fresh []message
	s.mu.Lock()
	previous := s.shown[config]
	for _, m := range messages {
		if _, dup := current[m.text]; dup {
			continue
		}
		current[m.text] = struct{}{}
		if _, already := previous[m.text]; !already {
			fresh = append(fresh, m)
		}
	}
	s.shown[config] = current
	s.mu.Unlock()

	for _, m := range fresh {
		s.notify(lsproto.WindowShowMessageInfo.NewNotificationMessage(&lsproto.ShowMessageParams{
			Type:    m.kind,
			Message: m.text,
		}))
	}
}

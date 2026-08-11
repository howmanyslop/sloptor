package flamework

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
)

type jsonSyntaxError struct {
	message string
}

func (e *jsonSyntaxError) Error() string {
	return e.message
}

func validateJSONSyntax(data []byte) error {
	var document json.RawMessage
	err := json.Unmarshal(data, &document)
	if err == nil {
		return nil
	}
	var syntaxError *json.SyntaxError
	if !errors.As(err, &syntaxError) {
		return err
	}
	position := int(syntaxError.Offset)
	line, column := jsonLineAndColumn(data, position)
	message := syntaxError.Error()
	trimmed := bytes.TrimSpace(data)
	if message == "unexpected end of JSON input" && len(trimmed) > 0 && trimmed[len(trimmed)-1] == '{' {
		message = "Expected property name or '}'"
	}
	return &jsonSyntaxError{message: fmt.Sprintf("SyntaxError: %s in JSON at position %d (line %d column %d)", message, position, line, column)}
}

func jsonLineAndColumn(data []byte, position int) (int, int) {
	line, column := 1, 1
	for index := 0; index < position && index < len(data); index++ {
		if data[index] == '\n' {
			line, column = line+1, 1
			continue
		}
		column++
	}
	return line, column
}

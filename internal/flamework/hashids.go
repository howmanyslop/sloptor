package flamework

import (
	"fmt"
	"math"
)

const (
	maxSafeHashID  = uint64(1<<53 - 1)
	hashAlphabet   = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ1234567890"
	hashSeparators = "cfhistuCFHISTU"
)

// EncodeHashID reproduces new Hashids(salt, 2).encode(id) from hashids 2.2.8.
func EncodeHashID(salt string, id uint64) (string, error) {
	if id > maxSafeHashID {
		return "", fmt.Errorf("flamework hash ID %d exceeds JavaScript's maximum safe integer %d", id, maxSafeHashID)
	}

	alphabet := withoutRunes([]rune(hashAlphabet), []rune(hashSeparators))
	separators := shuffleRunes(onlyRunes([]rune(hashSeparators), []rune(hashAlphabet)), []rune(salt))
	if len(separators) == 0 || float64(len(alphabet))/float64(len(separators)) > 3.5 {
		separatorCount := int(math.Ceil(float64(len(alphabet)) / 3.5))
		if separatorCount > len(separators) {
			difference := separatorCount - len(separators)
			alphabet = alphabet[difference:]
		}
	}

	alphabet = shuffleRunes(alphabet, []rune(salt))
	guardCount := int(math.Ceil(float64(len(alphabet)) / 12))
	alphabet = alphabet[guardCount:]

	number := float64(id)
	numbersID := math.Mod(number, 100)
	lottery := alphabet[int(math.Mod(numbersID, float64(len(alphabet))))]
	buffer := append([]rune{lottery}, []rune(salt)...)
	buffer = append(buffer, alphabet...)
	alphabet = shuffleRunes(alphabet, buffer)

	return string(append([]rune{lottery}, toHashAlphabet(number, alphabet)...)), nil
}

func shuffleRunes(alphabet, salt []rune) []rune {
	if len(salt) == 0 {
		return alphabet
	}

	shuffled := append([]rune(nil), alphabet...)
	for i, valueIndex, sum := len(shuffled)-1, 0, 0; i > 0; i, valueIndex = i-1, valueIndex+1 {
		valueIndex %= len(salt)
		integer := int(salt[valueIndex])
		sum += integer
		j := (integer + valueIndex + sum) % i
		shuffled[i], shuffled[j] = shuffled[j], shuffled[i]
	}
	return shuffled
}

func toHashAlphabet(input float64, alphabet []rune) []rune {
	encoded := make([]rune, 0, 12)
	base := float64(len(alphabet))
	for {
		encoded = append(encoded, alphabet[int(math.Mod(input, base))])
		input = math.Floor(input / base)
		if input <= 0 {
			break
		}
	}
	for left, right := 0, len(encoded)-1; left < right; left, right = left+1, right-1 {
		encoded[left], encoded[right] = encoded[right], encoded[left]
	}
	return encoded
}

func withoutRunes(input, excluded []rune) []rune {
	result := make([]rune, 0, len(input))
	for _, candidate := range input {
		if !containsRune(excluded, candidate) {
			result = append(result, candidate)
		}
	}
	return result
}

func onlyRunes(input, allowed []rune) []rune {
	result := make([]rune, 0, len(input))
	for _, candidate := range input {
		if containsRune(allowed, candidate) {
			result = append(result, candidate)
		}
	}
	return result
}

func containsRune(input []rune, candidate rune) bool {
	for _, item := range input {
		if item == candidate {
			return true
		}
	}
	return false
}

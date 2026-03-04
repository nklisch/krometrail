// Nested function calls for call stack testing.
// Equivalent to python/function-calls.py.
package main

import "fmt"

func add(a, b int) int {
	return a + b
}

func multiply(a, b int) int {
	result := 0
	for i := 0; i < b; i++ {
		result = add(result, a)
	}
	return result
}

func calculate(x, y int) int {
	product := multiply(x, y)
	sum := add(product, 10)
	return sum
}

func main() {
	answer := calculate(5, 3)
	fmt.Printf("Answer: %d\n", answer)
}

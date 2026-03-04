// Simple loop for basic stepping and variable inspection.
// Equivalent to python/simple-loop.py.
package main

import "fmt"

func sumRange(n int) int {
	total := 0
	for i := 0; i < n; i++ {
		total += i
	}
	return total
}

func main() {
	result := sumRange(10)
	fmt.Printf("Sum: %d\n", result)
}

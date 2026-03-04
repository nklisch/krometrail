/**
 * Simple loop for basic stepping and variable inspection.
 * Equivalent to python/simple-loop.py.
 */
function sumRange(n) {
	let total = 0;
	for (let i = 0; i < n; i++) {
		total += i;
	}
	return total;
}

const result = sumRange(10);
console.log(`Sum: ${result}`);

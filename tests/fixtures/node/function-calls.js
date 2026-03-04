/**
 * Nested function calls for call stack testing.
 * Equivalent to python/function-calls.py.
 */
function add(a, b) {
	return a + b;
}

function multiply(a, b) {
	let result = 0;
	for (let i = 0; i < b; i++) {
		result = add(result, a);
	}
	return result;
}

function calculate(x, y) {
	const product = multiply(x, y);
	const sum = add(product, 10);
	return sum;
}

const answer = calculate(5, 3);
console.log(`Answer: ${answer}`);

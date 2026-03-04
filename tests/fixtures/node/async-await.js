/**
 * Async/await for testing async stack traces.
 */
async function fetchData(id) {
	const data = { id, name: `item-${id}`, value: id * 10 };
	return data;
}

async function processItems(ids) {
	const results = [];
	for (const id of ids) {
		const data = await fetchData(id);
		results.push(data);
	}
	return results;
}

async function main() {
	const items = await processItems([1, 2, 3]);
	console.log(`Processed ${items.length} items`);
}

main();

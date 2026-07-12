(() => {
	const canvas = document.getElementById('scene');
	const counter = document.getElementById('counter');
	const context = canvas.getContext('2d');
	let sequence = 0;
	window.cdpTransportGate = {
		sequence: () => sequence,
		marker: (token) => console.log('cdp-transport-gate:' + token),
	};
	function draw(timestamp) {
		sequence += 1;
		const phase = (timestamp / 1000) % 1;
		context.fillStyle = '#101827';
		context.fillRect(0, 0, canvas.width, canvas.height);
		context.fillStyle = '#7dd3fc';
		context.beginPath();
		context.arc(160 + Math.cos(phase * Math.PI * 2) * 90, 90, 24, 0, Math.PI * 2);
		context.fill();
		counter.textContent = 'frame: ' + sequence;
		requestAnimationFrame(draw);
	}
	requestAnimationFrame(draw);
})();

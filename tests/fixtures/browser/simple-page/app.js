// Simple client-side app for browser recording tests

document.getElementById('error-btn').addEventListener('click', function() {
  // Throw an unhandled exception
  setTimeout(function() {
    throw new Error('Test unhandled exception from error button');
  }, 0);
  console.error('Error button clicked');
});

document.getElementById('fetch-btn').addEventListener('click', async function() {
  const output = document.getElementById('output');
  try {
    const resp = await fetch('/api/data');
    const data = await resp.json();
    output.textContent = JSON.stringify(data);
  } catch (err) {
    output.textContent = 'Fetch failed: ' + err.message;
  }
});

document.getElementById('login-form').addEventListener('submit', async function(e) {
  e.preventDefault();
  const username = document.getElementById('username').value;
  const password = document.getElementById('password').value;

  try {
    const resp = await fetch('/api/submit', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ username, password })
    });
    const result = await resp.json();
    document.getElementById('output').textContent = JSON.stringify(result);
  } catch (err) {
    console.error('Submit failed:', err);
  }
});

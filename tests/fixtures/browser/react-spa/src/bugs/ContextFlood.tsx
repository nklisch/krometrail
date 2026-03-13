import React, { createContext, useContext, useEffect, useState } from "react";

interface ThemeContextValue {
	theme: string;
	toggleTheme: () => void;
}

const ThemeContext = createContext<ThemeContextValue>({ theme: "light", toggleTheme: () => {} });

// 30 consumer components — all re-render on every context change
const consumers = Array.from({ length: 30 }, (_, i) => {
	function Consumer() {
		const { theme } = useContext(ThemeContext);
		return <span data-testid={`consumer-${i}`} style={{ display: "none" }}>{theme}</span>;
	}
	Consumer.displayName = `ThemeConsumer${i}`;
	return Consumer;
});

export function ContextFlood() {
	const [theme, setTheme] = useState("light");
	const [active, setActive] = useState(false);

	useEffect(() => {
		window.__TEST_CONTROLS__ = window.__TEST_CONTROLS__ || {};
		window.__TEST_CONTROLS__.activateContextFlood = () => setActive(true);
	}, []);

	const handleToggle = () => setTheme((t) => (t === "light" ? "dark" : "light"));

	// Intentional bug: object literal in render = new ref every render
	// This causes all 30 consumers to re-render on every parent render
	return (
		<ThemeContext.Provider value={{ theme, toggleTheme: handleToggle }}>
			<div data-testid="context-flood">
				<p>Theme: {theme}</p>
				{active && (
					<>
						<button type="button" data-testid="toggle-theme" onClick={handleToggle}>
							Toggle Theme
						</button>
						{consumers.map((Consumer, i) => (
							<Consumer key={i} />
						))}
					</>
				)}
			</div>
		</ThemeContext.Provider>
	);
}

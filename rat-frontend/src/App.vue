<template>
    <div id="app-shell">
        <header class="app-header">
            <router-link to="/" class="logo">Operator Control Panel</router-link>
        </header>
        <main class="app-main">
            <router-view />
        </main>
    </div>
</template>

<style>
:root {
    --bg: #0d1117;
    --bg-surface: #161b22;
    --bg-elevated: #1c2129;
    --border: #30363d;
    --text: #e6edf3;
    --text-muted: #8b949e;
    --accent: #58a6ff;
    --accent-hover: #79b8ff;
    --danger: #f85149;
    --success: #3fb950;
    --warning: #d29922;
    --radius: 6px;
    --font-mono: "SF Mono", "Cascadia Code", "Fira Code", Consolas, monospace;
}

* {
    margin: 0;
    padding: 0;
    box-sizing: border-box;
}

html,
body,
#app {
    height: 100%;
    background: var(--bg);
    color: var(--text);
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Helvetica, Arial, sans-serif;
    font-size: 14px;
}

#app-shell {
    display: flex;
    flex-direction: column;
    height: 100%;
}

.app-header {
    display: flex;
    align-items: center;
    padding: 0 16px;
    height: 48px;
    background: var(--bg-surface);
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
}

.logo {
    color: var(--text);
    font-weight: 600;
    font-size: 15px;
    text-decoration: none;
    letter-spacing: -0.3px;
}

.logo:hover {
    color: var(--accent);
}

.app-main {
    flex: 1;
    min-height: 0;
    overflow: hidden;
}
</style>

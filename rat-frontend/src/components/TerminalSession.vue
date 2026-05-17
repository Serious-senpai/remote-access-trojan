<template>
    <div class="terminal-wrapper" ref="wrapperEl"></div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch } from "vue";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import { api } from "../api/client";

const props = defineProps<{
    addr: string;
    sessionId: string;
}>();

const wrapperEl = ref<HTMLElement | null>(null);
let terminal: Terminal | null = null;
let fitAddon: FitAddon | null = null;
let eventSource: EventSource | null = null;
let resizeObserver: ResizeObserver | null = null;
let inputBuffer = "";
let sendTimer: ReturnType<typeof setTimeout> | null = null;

function setupTerminal() {
    if (!wrapperEl.value) return;

    terminal = new Terminal({
        convertEol: true,
        cursorBlink: true,
        fontSize: 13,
    });

    fitAddon = new FitAddon();
    terminal.loadAddon(fitAddon);
    terminal.open(wrapperEl.value);

    // Debounced fit
    requestAnimationFrame(() => fitAddon?.fit());

    // Handle user input — batch to avoid too many requests
    terminal.onData((data: string) => {
        inputBuffer += data;
        if (sendTimer) clearTimeout(sendTimer);
        sendTimer = setTimeout(flushInput, 32);
    });

    // Observe resize
    resizeObserver = new ResizeObserver(() => {
        requestAnimationFrame(() => fitAddon?.fit());
    });
    resizeObserver.observe(wrapperEl.value);
}

async function flushInput() {
    const data = inputBuffer;
    inputBuffer = "";
    sendTimer = null;
    if (!data) return;
    try {
        await api.POST("/clients/{addr}/sessions/{session_id}/data", {
            params: { path: { addr: props.addr, session_id: props.sessionId } },
            body: { type: "terminal-stdin", data },
        });
    } catch {
        terminal?.write("\r\n\x1b[31m[send error]\x1b[0m\r\n");
    }
}

function connectSSE() {
    const url = `/api/clients/${encodeURIComponent(props.addr)}/sessions/${encodeURIComponent(props.sessionId)}/data`;
    eventSource = new EventSource(url);

    eventSource.addEventListener("message", (ev: MessageEvent) => {
        try {
            const msg = JSON.parse(ev.data);
            if (msg.code !== "success" || !msg.data) return;
            const output = msg.data;
            if (output.type === "terminal-stdout" || output.type === "terminal-stderr") {
                terminal?.write(output.data);
            } else if (output.type === "terminal-closed") {
                terminal?.write("\r\n\x1b[90m[session closed]\x1b[0m\r\n");
                eventSource?.close();
            }
        } catch {
            // ignore parse errors
        }
    });

    eventSource.addEventListener("error", () => {
        // SSE auto-reconnects; we could show an indicator here
    });
}

// Fetch current terminal state to catch up on any buffered output
async function loadState() {
    try {
        const { data } = await api.GET("/clients/{addr}/sessions/{session_id}/state", {
            params: { path: { addr: props.addr, session_id: props.sessionId } },
        });
        if (data?.code === "success" && data.data?.type === "terminal" && data.data.data) {
            terminal?.write(data.data.data);
        }
    } catch {
        terminal?.write("\r\n\x1b[90m[failed to query current state]\x1b[0m\r\n");
    }
}

onMounted(async () => {
    setupTerminal();
    await loadState();
    connectSSE();
});

onUnmounted(() => {
    if (sendTimer) clearTimeout(sendTimer);
    eventSource?.close();
    resizeObserver?.disconnect();
    terminal?.dispose();
});

// Re-fit when visibility changes (e.g. tab switching)
watch(
    () => wrapperEl.value?.offsetParent,
    () => {
        requestAnimationFrame(() => fitAddon?.fit());
    }
);
</script>

<style scoped>
.terminal-wrapper {
    position: absolute;
    inset: 0;
    padding: 4px;
}

.terminal-wrapper :deep(.xterm) {
    height: 100%;
}

.terminal-wrapper :deep(.xterm-viewport) {
    overflow-y: auto !important;
}
</style>

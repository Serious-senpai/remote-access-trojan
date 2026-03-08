<template>
    <div class="client-view">
        <div class="view-header">
            <div class="breadcrumb">
                <router-link to="/" class="back-link">← Clients</router-link>
                <span class="separator">/</span>
                <span class="current">{{ addr }}</span>
            </div>
            <div class="header-actions">
                <button class="btn btn-primary" @click="createSession" :disabled="creatingSession">
                    {{ creatingSession ? "Creating…" : "+ New Terminal" }}
                </button>
            </div>
        </div>

        <div v-if="error" class="error-banner">{{ error }}</div>

        <!-- Client Info -->
        <div class="client-info-panel" v-if="clientInfo?.info">
            <div class="info-grid">
                <div class="info-item" v-if="clientInfo.info.host_name">
                    <span class="label">Host</span>
                    <span class="value">{{ clientInfo.info.host_name }}</span>
                </div>
                <div class="info-item" v-if="clientInfo.info.name">
                    <span class="label">OS</span>
                    <span class="value">{{ clientInfo.info.name }}</span>
                </div>
                <div class="info-item" v-if="clientInfo.info.long_os_version">
                    <span class="label">Version</span>
                    <span class="value">{{ clientInfo.info.long_os_version }}</span>
                </div>
                <div class="info-item" v-if="clientInfo.info.cpu_arch">
                    <span class="label">Arch</span>
                    <span class="value">{{ clientInfo.info.cpu_arch }}</span>
                </div>
                <div class="info-item" v-if="clientInfo.info.kernel_long_version">
                    <span class="label">Kernel</span>
                    <span class="value">{{ clientInfo.info.kernel_long_version }}</span>
                </div>
                <div class="info-item" v-if="clientInfo.info.physical_core_count">
                    <span class="label">CPU Cores</span>
                    <span class="value">{{ clientInfo.info.physical_core_count }}</span>
                </div>
                <div class="info-item" v-if="clientInfo.info.distribution_id">
                    <span class="label">Distro</span>
                    <span class="value">{{ clientInfo.info.distribution_id }}</span>
                </div>
            </div>
        </div>

        <!-- Sessions tabs -->
        <div class="sessions-container" v-if="sessions.length > 0">
            <div class="session-tabs">
                <button v-for="session in sessions" :key="session.id" class="session-tab"
                    :class="{ active: activeSessionId === session.id }" @click="activeSessionId = session.id">
                    <span class="tab-label">
                        Terminal {{ shortId(session.id) }}
                        <span class="tab-pid" v-if="session.inner.type === 'terminal'">
                            PID {{ session.inner.pid }}
                        </span>
                    </span>
                    <span class="tab-close" role="button" tabindex="0" @click.stop="deleteSession(session.id)"
                        @keydown.enter.stop="deleteSession(session.id)" title="Close session">×</span>
                </button>
            </div>
            <div class="session-content">
                <TerminalSession v-for="session in sessions" :key="session.id" v-show="activeSessionId === session.id"
                    :addr="addr" :session-id="session.id" />
            </div>
        </div>

        <div v-else-if="!loading" class="empty-state">
            No active sessions. Click <strong>+ New Terminal</strong> to start one.
        </div>
    </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted } from "vue";
import { api } from "../api/client";
import type { components } from "../api/types";
import TerminalSession from "../components/TerminalSession.vue";

type ClientAPI = components["schemas"]["ClientAPI"];
type SessionMetadata = components["schemas"]["SessionMetadata"];

const props = defineProps<{ addr: string }>();

const clientInfo = ref<ClientAPI | null>(null);
const sessions = ref<SessionMetadata[]>([]);
const activeSessionId = ref<string | null>(null);
const loading = ref(true);
const error = ref<string | null>(null);
const creatingSession = ref(false);
let pollTimer: ReturnType<typeof setInterval> | undefined;

function shortId(id: string): string {
    return id.slice(-4);
}

async function loadClient() {
    try {
        const { data } = await api.GET("/clients/{addr}", { params: { path: { addr: props.addr } } });
        if (data?.code === "success" && data.data) {
            clientInfo.value = data.data;
        }
    } catch {
        // non-critical
    }
}

async function loadSessions() {
    try {
        const { data } = await api.GET("/clients/{addr}/sessions", { params: { path: { addr: props.addr } } });
        if (data?.code === "success" && data.data) {
            const oldIds = new Set(sessions.value.map((s) => s.id));
            sessions.value = data.data;
            // auto-select first if none selected or active was removed
            if (!activeSessionId.value || !data.data.some((s) => s.id === activeSessionId.value)) {
                activeSessionId.value = data.data[0]?.id ?? null;
            }
            // auto-select newly created sessions
            for (const s of data.data) {
                if (!oldIds.has(s.id) && oldIds.size > 0) {
                    activeSessionId.value = s.id;
                }
            }
        }
    } catch (e) {
        error.value = String(e);
    }
}

async function createSession() {
    creatingSession.value = true;
    error.value = null;
    try {
        const { data } = await api.POST("/clients/{addr}/sessions", {
            params: { path: { addr: props.addr } },
            body: "terminal",
        });
        if (data?.code === "success" && data.data) {
            activeSessionId.value = data.data.id;
            await loadSessions();
        } else {
            error.value = data?.error ?? "Failed to create session";
        }
    } catch (e) {
        error.value = String(e);
    } finally {
        creatingSession.value = false;
    }
}

async function deleteSession(sessionId: string) {
    try {
        await api.DELETE("/clients/{addr}/sessions/{session_id}", {
            params: { path: { addr: props.addr, session_id: sessionId } },
        });
        await loadSessions();
    } catch (e) {
        error.value = String(e);
    }
}

onMounted(async () => {
    await Promise.all([loadClient(), loadSessions()]);
    loading.value = false;
    pollTimer = setInterval(loadSessions, 5000);
});

onUnmounted(() => {
    clearInterval(pollTimer);
});
</script>

<style scoped>
.client-view {
    display: flex;
    flex-direction: column;
    height: 100%;
    padding: 16px;
}

.view-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 16px;
    flex-shrink: 0;
}

.breadcrumb {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 14px;
}

.back-link {
    color: var(--accent);
    text-decoration: none;
}

.back-link:hover {
    text-decoration: underline;
}

.separator {
    color: var(--text-muted);
}

.current {
    font-family: var(--font-mono);
    color: var(--text);
    font-weight: 600;
}

.btn {
    padding: 6px 14px;
    border-radius: var(--radius);
    font-size: 13px;
    cursor: pointer;
    border: 1px solid var(--border);
    background: var(--bg-surface);
    color: var(--text);
    transition: background 0.15s;
}

.btn:hover:not(:disabled) {
    background: var(--bg-elevated);
}

.btn:disabled {
    opacity: 0.5;
    cursor: default;
}

.btn-primary {
    background: var(--accent);
    border-color: var(--accent);
    color: #000;
    font-weight: 600;
}

.btn-primary:hover:not(:disabled) {
    background: var(--accent-hover);
}

.error-banner {
    background: rgba(248, 81, 73, 0.1);
    border: 1px solid var(--danger);
    color: var(--danger);
    padding: 10px 14px;
    border-radius: var(--radius);
    margin-bottom: 12px;
    font-size: 13px;
    flex-shrink: 0;
}

.client-info-panel {
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 14px;
    margin-bottom: 16px;
    flex-shrink: 0;
}

.info-grid {
    display: flex;
    flex-wrap: wrap;
    gap: 16px 32px;
}

.info-item {
    display: flex;
    flex-direction: column;
    gap: 2px;
}

.info-item .label {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--text-muted);
}

.info-item .value {
    font-size: 13px;
    font-family: var(--font-mono);
}

.sessions-container {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
}

.session-tabs {
    display: flex;
    gap: 2px;
    background: var(--bg);
    border-bottom: 1px solid var(--border);
    overflow-x: auto;
    flex-shrink: 0;
}

.session-tab {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-bottom: none;
    border-radius: var(--radius) var(--radius) 0 0;
    color: var(--text-muted);
    cursor: pointer;
    font-size: 12px;
    white-space: nowrap;
    transition: background 0.15s, color 0.15s;
}

.session-tab:hover {
    background: var(--bg-elevated);
    color: var(--text);
}

.session-tab.active {
    background: var(--bg-elevated);
    color: var(--text);
    border-color: var(--accent);
    border-bottom: 1px solid var(--bg-elevated);
    margin-bottom: -1px;
}

.tab-label {
    display: flex;
    align-items: center;
    gap: 6px;
}

.tab-pid {
    color: var(--text-muted);
    font-size: 11px;
}

.tab-close {
    background: none;
    border: none;
    color: var(--text-muted);
    cursor: pointer;
    font-size: 16px;
    line-height: 1;
    padding: 0 2px;
    border-radius: 3px;
}

.tab-close:hover {
    background: rgba(248, 81, 73, 0.2);
    color: var(--danger);
}

.session-content {
    flex: 1;
    min-height: 0;
    position: relative;
    overflow: hidden;
    border: 1px solid var(--border);
    border-top: none;
    border-radius: 0 0 var(--radius) var(--radius);
    background: #000;
}

.empty-state {
    color: var(--text-muted);
    text-align: center;
    padding: 48px 0;
    font-size: 15px;
}
</style>

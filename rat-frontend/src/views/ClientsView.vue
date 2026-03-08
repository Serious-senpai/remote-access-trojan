<template>
    <div class="clients-view">
        <div class="view-header">
            <h1>Clients</h1>
            <button class="btn btn-ghost" @click="refresh" :disabled="loading">
                {{ loading ? "Loading…" : "Refresh" }}
            </button>
        </div>

        <div v-if="error" class="error-banner">{{ error }}</div>

        <div v-if="!loading && clients.length === 0" class="empty-state">
            No clients connected.
        </div>

        <div class="client-grid" v-if="clients.length > 0">
            <router-link v-for="client in clients" :key="client.address"
                :to="{ name: 'client', params: { addr: client.address } }" class="client-card">
                <div class="client-address">{{ client.address }}</div>
                <div class="client-info" v-if="client.info">
                    <div class="info-row" v-if="client.info.host_name">
                        <span class="label">Host</span>
                        <span class="value">{{ client.info.host_name }}</span>
                    </div>
                    <div class="info-row" v-if="client.info.name">
                        <span class="label">OS</span>
                        <span class="value">{{ client.info.name }}</span>
                    </div>
                    <div class="info-row" v-if="client.info.cpu_arch">
                        <span class="label">Arch</span>
                        <span class="value">{{ client.info.cpu_arch }}</span>
                    </div>
                    <div class="info-row" v-if="client.info.kernel_version">
                        <span class="label">Kernel</span>
                        <span class="value">{{ client.info.kernel_version }}</span>
                    </div>
                    <div class="info-row" v-if="client.info.physical_core_count">
                        <span class="label">Cores</span>
                        <span class="value">{{ client.info.physical_core_count }}</span>
                    </div>
                </div>
                <div class="client-no-info" v-else>No system info available</div>
            </router-link>
        </div>
    </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted } from "vue";
import { api } from "../api/client";
import type { components } from "../api/types";

type ClientAPI = components["schemas"]["ClientAPI"];

const clients = ref<ClientAPI[]>([]);
const loading = ref(false);
const error = ref<string | null>(null);
let pollTimer: ReturnType<typeof setInterval> | undefined;

async function refresh() {
    loading.value = true;
    error.value = null;
    try {
        const { data } = await api.GET("/clients");
        if (data?.code === "success" && data.data) {
            clients.value = data.data;
        } else {
            error.value = data?.error ?? "Failed to load clients";
        }
    } catch (e) {
        error.value = String(e);
    } finally {
        loading.value = false;
    }
}

onMounted(() => {
    refresh();
    pollTimer = setInterval(refresh, 5000);
});

onUnmounted(() => {
    clearInterval(pollTimer);
});
</script>

<style scoped>
.clients-view {
    padding: 24px;
    max-width: 960px;
    margin: 0 auto;
}

.view-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 20px;
}

.view-header h1 {
    font-size: 20px;
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

.btn-ghost {
    background: transparent;
}

.error-banner {
    background: rgba(248, 81, 73, 0.1);
    border: 1px solid var(--danger);
    color: var(--danger);
    padding: 10px 14px;
    border-radius: var(--radius);
    margin-bottom: 16px;
    font-size: 13px;
}

.empty-state {
    color: var(--text-muted);
    text-align: center;
    padding: 48px 0;
    font-size: 15px;
}

.client-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: 12px;
}

.client-card {
    display: block;
    text-decoration: none;
    color: var(--text);
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 16px;
    transition: border-color 0.15s, background 0.15s;
}

.client-card:hover {
    border-color: var(--accent);
    background: var(--bg-elevated);
}

.client-address {
    font-family: var(--font-mono);
    font-size: 14px;
    font-weight: 600;
    margin-bottom: 10px;
    color: var(--accent);
}

.client-info {
    display: flex;
    flex-direction: column;
    gap: 4px;
}

.info-row {
    display: flex;
    justify-content: space-between;
    font-size: 12px;
}

.info-row .label {
    color: var(--text-muted);
}

.info-row .value {
    color: var(--text);
    font-family: var(--font-mono);
}

.client-no-info {
    color: var(--text-muted);
    font-size: 12px;
}
</style>

<script setup lang="ts">
import { ref, onMounted } from "vue";
import { api } from "./api/client";

const loading = ref(true);
const error = ref<string | null>(null);
const clients = ref<any[]>([]);

onMounted(async () => {
  const { data, error: apiError } = await api.GET("/api/clients");

  if (apiError) {
    error.value = JSON.stringify(apiError);
  } else if (data) {
    clients.value = data.data ?? [];
  }

  loading.value = false;
});
</script>

<template>
  <div style="padding: 20px; font-family: sans-serif">
    <h1>Clients</h1>

    <div v-if="loading">Loading...</div>

    <div v-else-if="error">
      <h3 style="color: red">Error:</h3>
      <pre>{{ error }}</pre>
    </div>

    <div v-else>
      <div v-if="clients.length === 0">No clients found.</div>

      <div v-for="(client, index) in clients" :key="index"
           style="border: 1px solid #ccc; padding: 10px; margin-bottom: 10px">

        <div><strong>Address:</strong> {{ client.address }}</div>

        <div style="margin-top: 5px">
          <strong>Host:</strong> {{ client.info?.host_name }}<br />
          <strong>OS:</strong> {{ client.info?.name }} {{ client.info?.os_version }}<br />
          <strong>Kernel:</strong> {{ client.info?.kernel_version }}<br />
          <strong>CPU Arch:</strong> {{ client.info?.cpu_arch }}
        </div>

      </div>
    </div>
  </div>
</template>

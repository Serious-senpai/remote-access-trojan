import vue from "@vitejs/plugin-vue";
import fs from "node:fs";
import { defineConfig } from "vite";

const backend = {
  target: "https://localhost:12111",
  changeOrigin: true,
  secure: false,
};

// https://vite.dev/config/
export default defineConfig(({ command }) => {
  return {
    plugins: [vue()],
    server: command === "serve" ? {
      https: {
        key: fs.readFileSync("../certs/cert.key.pem"),
        cert: fs.readFileSync("../certs/cert.pem"),
      },
      proxy: {
        "/api": backend,
        "/docs": backend,
      },
    } : undefined,
  };
})

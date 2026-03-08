import { createRouter, createWebHistory } from "vue-router";

const router = createRouter({
    history: createWebHistory(),
    routes: [
        {
            path: "/",
            name: "clients",
            component: () => import("../views/ClientsView.vue"),
        },
        {
            path: "/clients/:addr",
            name: "client",
            component: () => import("../views/ClientView.vue"),
            props: true,
        },
    ],
});

export default router;

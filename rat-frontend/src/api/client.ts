import createClient from "openapi-fetch";
import type { paths } from "./types";

export const api = createClient<paths>({
    baseUrl: "", // empty for proxy to work
});

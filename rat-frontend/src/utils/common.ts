export class RequestStatus {
    loading: boolean;
    error: string | null;

    constructor() {
        this.loading = false;
        this.error = null;
    }

    start() {
        this.loading = true;
        this.error = null;
    }
}

export function withDeadline<T>(promise: Promise<T>, milliseconds: number, message = "Timeout"): Promise<T> {
    return Promise.race([
        promise,
        new Promise<never>((_, reject) =>
            setTimeout(() => reject(new Error(message)), milliseconds)
        ),
    ]);
}
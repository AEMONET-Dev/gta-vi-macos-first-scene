// Costs of the Wine primitives the buffer refresh uses, measured in the game's
// Wine/GPTK bottle under Rosetta. Offline: no game, no GPU.
// Build: x86_64-w64-mingw32-clang -O2 wineprim.c -o wineprim.exe
#include <windows.h>
#include <stdio.h>
#include <string.h>

static double now_us(void) {
    static LARGE_INTEGER f;
    LARGE_INTEGER c;
    if (!f.QuadPart) QueryPerformanceFrequency(&f);
    QueryPerformanceCounter(&c);
    return (double)c.QuadPart * 1e6 / (double)f.QuadPart;
}

#define MB (1024ull * 1024ull)
#define BLOCK (64ull * 1024ull)

static BOOL rpm(void *dst, const void *src, SIZE_T n, SIZE_T *done) {
    SIZE_T d = 0;
    BOOL ok = ReadProcessMemory(GetCurrentProcess(), src, dst, n, &d);
    if (done) *done = d;
    return ok;
}

typedef struct { char *src; char *dst; SIZE_T len; int reps; double us; int mode; } Job;

static DWORD WINAPI worker(void *p) {
    Job *j = p;
    double t = now_us();
    for (int r = 0; r < j->reps; r++) {
        if (j->mode == 0) rpm(j->dst, j->src, j->len, NULL);
        else if (j->mode == 1) memcpy(j->dst, j->src, j->len);
        else if (j->mode == 2) memset(j->dst, 0, j->len);
        else if (j->mode == 3) {
            MEMORY_BASIC_INFORMATION mi;
            for (SIZE_T k = 0; k < j->len; k += BLOCK) VirtualQuery(j->src + k, &mi, sizeof mi);
        }
    }
    j->us = now_us() - t;
    return 0;
}

// `threads` workers each run `mode` `reps` times over their own `len` bytes.
// Returns aggregate GB/s (or per-call us for queries).
static void parallel(const char *name, int mode, int threads, SIZE_T len, int reps, char **srcs, char **dsts) {
    Job jobs[8];
    HANDLE h[8];
    double t = now_us();
    for (int i = 0; i < threads; i++) {
        jobs[i] = (Job){srcs[i], dsts[i], len, reps, 0, mode};
        h[i] = CreateThread(NULL, 0, worker, &jobs[i], 0, NULL);
    }
    WaitForMultipleObjects(threads, h, TRUE, INFINITE);
    double wall = now_us() - t;
    for (int i = 0; i < threads; i++) CloseHandle(h[i]);
    if (mode == 3) {
        double calls = (double)threads * reps * (len / BLOCK);
        printf("%-34s threads=%d  wall %8.1f ms  %.2f us/call (wall/calls*threads %.2f)\n", name, threads, wall / 1e3,
               jobs[0].us / ((double)reps * (len / BLOCK)), wall / calls * threads);
    } else {
        double gb = (double)threads * reps * len / 1e9;
        printf("%-34s threads=%d  wall %8.1f ms  aggregate %6.2f GB/s  per-thread %6.2f GB/s\n", name, threads,
               wall / 1e3, gb / (wall / 1e6), (double)reps * len / 1e9 / (jobs[0].us / 1e6));
    }
}

int main(void) {
    const SIZE_T LEN = 256 * MB;
    char *src[4], *dst[4];
    // Title memory is section views (XMem physical pages); host copies are upload maps.
    for (int i = 0; i < 4; i++) {
        HANDLE s = CreateFileMappingW(INVALID_HANDLE_VALUE, NULL, PAGE_READWRITE, 0, (DWORD)LEN, NULL);
        src[i] = MapViewOfFile(s, FILE_MAP_WRITE, 0, 0, LEN);
        dst[i] = VirtualAlloc(NULL, LEN, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE);
        memset(src[i], 0x5a + i, LEN);
        memset(dst[i], 1, LEN);
    }
    printf("== bulk copies, 256 MiB per thread, section view -> private memory\n");
    for (int t = 1; t <= 4; t *= 2) parallel("ReadProcessMemory whole span", 0, t, LEN, 4, src, dst);
    parallel("ReadProcessMemory whole span", 0, 4, LEN, 4, src, dst);
    for (int t = 1; t <= 4; t *= 2) parallel("memcpy (no kernel)", 1, t, LEN, 4, src, dst);
    parallel("memcpy (no kernel)", 1, 4, LEN, 4, src, dst);
    for (int t = 1; t <= 4; t *= 2) parallel("zero-fill memset", 2, t, LEN, 4, src, dst);
    parallel("zero-fill memset", 2, 4, LEN, 4, src, dst);

    printf("== small spans: ReadProcessMemory call overhead\n");
    {
        double t = now_us();
        int n = 20000;
        for (int k = 0; k < n; k++) rpm(dst[0], src[0] + (k % 1000) * 4096, 4096, NULL);
        printf("RPM 4 KiB x%d: %.2f us/call\n", n, (now_us() - t) / n);
        t = now_us();
        for (int k = 0; k < n; k++) rpm(dst[0], src[0] + (k % 100) * BLOCK, BLOCK, NULL);
        printf("RPM 64 KiB x%d: %.2f us/call\n", n, (now_us() - t) / n);
    }

    printf("== VirtualQuery\n");
    parallel("VirtualQuery per 64 KiB", 3, 1, LEN, 4, src, dst);
    parallel("VirtualQuery per 64 KiB", 3, 4, LEN, 4, src, dst);

    printf("== failed whole-span reads: span with one decommitted 64 KiB block\n");
    {
        SIZE_T span = 64 * MB;
        char *p = VirtualAlloc(NULL, span, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE);
        memset(p, 7, span);
        double ok_us;
        {
            double t = now_us();
            for (int r = 0; r < 8; r++) rpm(dst[0], p, span, NULL);
            ok_us = (now_us() - t) / 8;
        }
        printf("RPM 64 MiB, all mapped: %.2f ms\n", ok_us / 1e3);
        double fracs[] = {0.0, 0.25, 0.5, 0.75, 0.99};
        for (int k = 0; k < 5; k++) {
            SIZE_T at = ((SIZE_T)(span * fracs[k]) / BLOCK) * BLOCK;
            VirtualFree(p + at, BLOCK, MEM_DECOMMIT);
            SIZE_T done = 0;
            BOOL ok = 1;
            double t = now_us();
            for (int r = 0; r < 8; r++) ok = rpm(dst[0], p, span, &done);
            double us = (now_us() - t) / 8;
            // Did the kernel copy the bytes before the hole?
            memset(dst[0], 1, at ? at : 1);
            rpm(dst[0], p, span, NULL);
            int prefix_copied = at ? (dst[0][at - 1] == 7 && dst[0][0] == 7) : 0;
            printf("RPM 64 MiB, hole at %4.0f%%: ok=%d done=%zu  %.2f ms (%.0f%% of mapped)  prefix copied before failing: %s\n",
                   fracs[k] * 100, ok, done, us / 1e3, us / ok_us * 100, at ? (prefix_copied ? "yes" : "no") : "n/a");
            VirtualAlloc(p + at, BLOCK, MEM_COMMIT, PAGE_READWRITE);
            memset(p + at, 7, BLOCK);
        }
    }

    printf("== VirtualQuery latency while 3 threads run ReadProcessMemory\n");
    {
        Job jobs[3];
        HANDLE h[3];
        for (int i = 0; i < 3; i++) {
            jobs[i] = (Job){src[i + 1], dst[i + 1], LEN, 6, 0, 0};
            h[i] = CreateThread(NULL, 0, worker, &jobs[i], 0, NULL);
        }
        Sleep(20);
        MEMORY_BASIC_INFORMATION mi;
        double worst = 0, total = 0;
        int n = 0;
        double end = now_us() + 300000;
        while (now_us() < end) {
            double t = now_us();
            VirtualQuery(src[0] + (n % 4096) * BLOCK % LEN, &mi, sizeof mi);
            double d = now_us() - t;
            total += d;
            if (d > worst) worst = d;
            n++;
        }
        WaitForMultipleObjects(3, h, TRUE, INFINITE);
        printf("VirtualQuery during RPM: %d calls, mean %.2f us, worst %.1f us\n", n, total / n, worst);
        DWORD old;
        worst = 0; total = 0; n = 0;
        for (int i = 0; i < 3; i++) {
            jobs[i] = (Job){src[i + 1], dst[i + 1], LEN, 6, 0, 0};
            h[i] = CreateThread(NULL, 0, worker, &jobs[i], 0, NULL);
        }
        Sleep(20);
        end = now_us() + 300000;
        while (now_us() < end) {
            double t = now_us();
            VirtualProtect(src[0] + (n % 2048) * BLOCK, BLOCK, (n & 1) ? PAGE_READWRITE : PAGE_READONLY, &old);
            double d = now_us() - t;
            total += d;
            if (d > worst) worst = d;
            n++;
        }
        WaitForMultipleObjects(3, h, TRUE, INFINITE);
        printf("VirtualProtect during RPM: %d calls, mean %.2f us, worst %.1f us\n", n, total / n, worst);
        worst = 0; total = 0;
        for (int k = 0; k < 2000; k++) {
            double t = now_us();
            VirtualProtect(src[0] + (k % 2048) * BLOCK, BLOCK, (k & 1) ? PAGE_READWRITE : PAGE_READONLY, &old);
            double d = now_us() - t;
            total += d;
            if (d > worst) worst = d;
        }
        printf("VirtualProtect idle: 2000 calls, mean %.2f us, worst %.1f us\n", total / 2000, worst);
    }
    return 0;
}

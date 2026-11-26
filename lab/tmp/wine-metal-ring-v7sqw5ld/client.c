#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <stdio.h>
#include "layout.h"

static double ms(LARGE_INTEGER a, LARGE_INTEGER b, LARGE_INTEGER freq) {
    return (double)(b.QuadPart-a.QuadPart) * 1000.0 / (double)freq.QuadPart;
}
static LONG acquire(uint32_t *word) {
    return InterlockedCompareExchange((volatile LONG*)word, 0, 0);
}
int main(void) {
    HANDLE file = CreateFileW(L"arena.bin", GENERIC_READ | GENERIC_WRITE, FILE_SHARE_READ | FILE_SHARE_WRITE, NULL, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, NULL);
    if (file == INVALID_HANDLE_VALUE) { printf("open error=%lu\n", GetLastError()); return 2; }
    HANDLE map = CreateFileMappingW(file, NULL, PAGE_READWRITE, 0, 0, NULL);
    if (!map) return 3;
    void *view = MapViewOfFile(map, FILE_MAP_ALL_ACCESS, 0, 0, HEADER_BYTES + DATA_BYTES);
    if (!view) return 4;
    struct Control *ctrl = view;
    uint32_t *data = (uint32_t*)((char *)view + HEADER_BYTES);
    if ((uint32_t)acquire(&ctrl->magic) != MAGIC) return 5;
    LARGE_INTEGER freq;
    if (!QueryPerformanceFrequency(&freq)) return 6;
    printf("seq,roundtrip_ms,gpu_ms,fill_and_roundtrip_and_verify_ms\n");
    for (uint32_t nonce=1; nonce<=ROUNDS; ++nonce) {
        LARGE_INTEGER start, send, returned, checked;
        QueryPerformanceCounter(&start);
        for (uint32_t i=0; i<WORDS; ++i) data[i] = seed_word(i, nonce);
        const ULONGLONG timeout = GetTickCount64() + 25000;
        QueryPerformanceCounter(&send);
        InterlockedExchange((volatile LONG*)&ctrl->request, (LONG)nonce);
        while ((uint32_t)acquire(&ctrl->complete) != nonce) {
            if (acquire(&ctrl->failure) || GetTickCount64() > timeout) return 7;
            SwitchToThread();
        }
        QueryPerformanceCounter(&returned);
        for (uint32_t i=0; i<WORDS; ++i) {
            uint32_t expected = answer(seed_word(i, nonce), nonce);
            if (data[i] != expected) {
                printf("wrong seq=%u index=%u expected=%u saw=%u\n", nonce, i, expected, data[i]);
                return 8;
            }
        }
        QueryPerformanceCounter(&checked);
        printf("%u,%.6f,%.6f,%.6f\n", nonce, ms(send, returned, freq), ctrl->gpu_ms, ms(start, checked, freq));
    }
    printf("verified_words_per_round=%zu rounds=%u payload_bytes=%u\n", WORDS, ROUNDS, DATA_BYTES);
    UnmapViewOfFile(view); CloseHandle(map); CloseHandle(file);
    return 0;
}


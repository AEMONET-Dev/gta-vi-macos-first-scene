#pragma once
#include <stdint.h>
#define HEADER_BYTES 65536u
#define DATA_BYTES (16u * 1024u * 1024u)
#define WORDS (DATA_BYTES / sizeof(uint32_t))
#define WARMUPS 10u
#define SAMPLES 80u
#define ROUNDS (WARMUPS + SAMPLES)
#define MAGIC UINT32_C(0x4d354d42)
struct Control {
    uint32_t magic;
    uint32_t request;
    uint32_t complete;
    uint32_t failure;
    double gpu_ms;
};
static inline uint32_t seed_word(uint32_t index, uint32_t nonce) {
    return (index * UINT32_C(2654435761)) ^ (nonce * UINT32_C(2246822519));
}
static inline uint32_t answer(uint32_t x, uint32_t nonce) {
    return x * UINT32_C(1664525) + UINT32_C(1013904223) + nonce * 13u;
}


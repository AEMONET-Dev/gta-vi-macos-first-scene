#import <Foundation/Foundation.h>
#import <Metal/Metal.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <fcntl.h>
#include <unistd.h>
#include <sched.h>
#include <time.h>
#include "layout.h"

int main(int argc, const char *argv[]) {
  @autoreleasepool {
    if (argc != 2) return 1;
    size_t total = (size_t)HEADER_BYTES + DATA_BYTES;
    int fd = open(argv[1], O_RDWR | O_CREAT | O_EXCL, 0600);
    if (fd < 0 || ftruncate(fd, total) != 0) { perror("create"); return 2; }
    void *base = mmap(NULL, total, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
    close(fd);
    if (base == MAP_FAILED) { perror("mmap"); return 3; }
    struct Control *ctrl = base;
    void *pixels = (char *)base + HEADER_BYTES;
    id<MTLDevice> dev = MTLCreateSystemDefaultDevice();
    if (!dev || !dev.hasUnifiedMemory) return 4;
    // The operating system owns this MAP_SHARED view until this process exits.
    id<MTLBuffer> buf = [dev newBufferWithBytesNoCopy:pixels length:DATA_BYTES options:MTLResourceStorageModeShared deallocator:nil];
    if (!buf || buf.contents != pixels || buf.length != DATA_BYTES) return 5;
    NSString *msl = @"#include <metal_stdlib>\nusing namespace metal; kernel void round_trip(device uint* d [[buffer(0)]], constant uint& nonce [[buffer(1)]], uint i [[thread_position_in_grid]]) { d[i] = d[i] * 1664525u + 1013904223u + nonce * 13u; }";
    NSError *err = nil;
    id<MTLLibrary> lib = [dev newLibraryWithSource:msl options:nil error:&err];
    if (!lib) { fprintf(stderr, "%s\n", err.description.UTF8String); return 6; }
    id<MTLComputePipelineState> pso = [dev newComputePipelineStateWithFunction:[lib newFunctionWithName:@"round_trip"] error:&err];
    if (!pso) { fprintf(stderr, "%s\n", err.description.UTF8String); return 7; }
    id<MTLCommandQueue> queue = [dev newCommandQueue];
    if (!queue) return 8;
    __atomic_store_n(&ctrl->magic, MAGIC, __ATOMIC_RELEASE);
    printf("ready gpu=%s shared_bytes=%u word_count=%zu mapped_pointer_matches_MTLBuffer=%d\n", dev.name.UTF8String, DATA_BYTES, WORDS, buf.contents==pixels);
    fflush(stdout);
    for (uint32_t i=1; i<=ROUNDS; ++i) {
      @autoreleasepool {
        const time_t timeout = time(NULL) + 25;
        while (__atomic_load_n(&ctrl->request, __ATOMIC_ACQUIRE) != i) {
          if (time(NULL) >= timeout) { fprintf(stderr, "timeout waiting for %u\n", i); return 9; }
          sched_yield();
        }
        id<MTLCommandBuffer> cmd = [queue commandBuffer];
        id<MTLComputeCommandEncoder> enc = [cmd computeCommandEncoder];
        [enc setComputePipelineState:pso];
        [enc setBuffer:buf offset:0 atIndex:0];
        [enc setBytes:&i length:sizeof(i) atIndex:1];
        [enc dispatchThreads:MTLSizeMake(WORDS,1,1) threadsPerThreadgroup:MTLSizeMake(256,1,1)];
        [enc endEncoding];
        [cmd commit];
        [cmd waitUntilCompleted];
        if (cmd.status != MTLCommandBufferStatusCompleted || cmd.GPUEndTime < cmd.GPUStartTime || cmd.GPUStartTime <= 0) {
          fprintf(stderr, "bad completion %s\n", cmd.error.description.UTF8String);
          __atomic_store_n(&ctrl->failure, 1, __ATOMIC_RELEASE);
          return 10;
        }
        ctrl->gpu_ms = 1000.0 * (cmd.GPUEndTime - cmd.GPUStartTime);
        __atomic_store_n(&ctrl->complete, i, __ATOMIC_RELEASE);
      }
    }
    printf("completed %u commands\n", ROUNDS);
  }
  return 0;
}


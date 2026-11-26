#include <Metal/Metal.hpp>
#include <Foundation/Foundation.hpp>
#include <algorithm>
#include <chrono>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <sys/mman.h>
#include <unistd.h>
#include <utility>
#include <vector>

// The harness supplies the emulator's memory, trace hook and device provider.
// The production methods themselves are included without modifying their body.
namespace cvars { bool shared_memory_zero_copy; }
namespace xe::memory { size_t page_size() { return getpagesize(); } }
namespace ui::metal {
struct MetalProvider {
  MTL::Device *device;
  MTL::Device *GetDevice() const { return device; }
};
}
struct Processor {
  ui::metal::MetalProvider provider;
  ui::metal::MetalProvider &GetMetalProvider() { return provider; }
};
struct Guest {
  uint8_t *physical;
  void *TranslatePhysical(size_t offset) { return physical + offset; }
};
struct Trace { void WriteMemoryRead(uint32_t,uint32_t) {} };
#define XELOGE(...) fprintf(stderr,__VA_ARGS__)
#define XELOGD(...)
#define XELOGW(...)
#define SCOPE_profile_cpu_f(...)
using PageRange = std::pair<uint32_t,uint32_t>;
struct MetalSharedMemory {
  static constexpr uint32_t kBufferSize = 174 * 1024 * 1024;  // Captured resource size.
  Processor command_processor_;
  Guest guest;
  Trace trace_writer_;
  MTL::Buffer *buffer_ = nullptr;
  bool use_zero_copy_ = false;
  std::vector<PageRange> spans;
  void InitializeCommon() {}
  Guest &memory() { return guest; }
  unsigned page_size_log2() { return 12; }
  void MakeRangeValid(uint32_t start,uint32_t size,bool) { spans.emplace_back(start,size); }
  bool Initialize();
  bool UploadRanges(const PageRange *,uint32_t);
};
#include "xenia-methods.inc"
using Clock = std::chrono::steady_clock;
int main() {
  auto *pool = NS::AutoreleasePool::alloc()->init();
  auto *device = MTL::CreateSystemDefaultDevice();
  if (!device) return 1;
  const size_t bytes = MetalSharedMemory::kBufferSize;
  auto *src = static_cast<uint8_t *>(mmap(nullptr,bytes,PROT_READ|PROT_WRITE,MAP_ANON|MAP_PRIVATE,-1,0));
  if (src == MAP_FAILED) return 2;
  auto *q = device->newCommandQueue();
  auto *readback = device->newBuffer(bytes,MTL::ResourceStorageModeShared);
  std::vector<uint8_t> expected(bytes);
  // Synthetic overlapping, adjacent, disjoint and empty intervals. Include the 27 MiB seam.
  const std::vector<PageRange> ranges{{1,2},{2,3},{5,1},{6911,2},{44543,1}};
  auto capture = [&](MTL::Buffer *buffer) {
    auto *cmd = q->commandBuffer();
    auto *enc = cmd->blitCommandEncoder();
    enc->copyFromBuffer(buffer,0,readback,0,bytes);
    enc->endEncoding(); cmd->commit(); cmd->waitUntilCompleted();
    if (cmd->status() != MTL::CommandBufferStatusCompleted) std::exit(6);
  };
  printf("device=%s bytes=%zu\n",device->name()->utf8String(),bytes);
  for (bool alias : {false,true}) {
    for (size_t i = 0; i < bytes; ++i) src[i] = uint8_t((i >> 12) * 37u + i);
    cvars::shared_memory_zero_copy = alias;
    MetalSharedMemory mem{{{device}},{src}};
    if (!mem.Initialize() || mem.use_zero_copy_ != alias || !mem.UploadRanges(nullptr,0)) return 3;
    for (unsigned round = 0; round < 8; ++round) {
      for (const auto &[page,count] : ranges) {
        const size_t offset = page * 4096;
        for (size_t i = 0; i < count * 4096; ++i) src[offset+i] ^= uint8_t(43+round);
      }
      mem.spans.clear();
      auto start = Clock::now();
      if (!mem.UploadRanges(ranges.data(),(uint32_t)ranges.size())) return 4;
      double us = std::chrono::duration<double,std::micro>(Clock::now()-start).count();
      if (mem.spans != std::vector<PageRange>{{4096,20480},{6911 * 4096,8192},{44543 * 4096,4096}}) return 5;
      capture(mem.buffer_);
      bool equal = !memcmp(src,readback->contents(),bytes);
      printf("mode=%s round=%u known_span_apply_us=%.3f equal=%d\n",alias?"alias":"copy",round,us,equal);
      if (!equal) return 7;
    }
    memcpy(expected.data(),src,bytes);
    src[27*1024*1024] ^= 1; // Deliberately change one byte after the version was chosen.
    capture(mem.buffer_);
    auto *seen = static_cast<const uint8_t *>(readback->contents());
    size_t diff = 0;
    for (size_t i = 0; i < bytes; ++i) diff += expected[i] != seen[i];
    printf("mode=%s changed_after_selection=%zu expected=%d\n",alias?"alias":"copy",diff,alias?1:0);
    if (diff != (alias ? 1u : 0u)) return 8;
    mem.buffer_->release();
  }
  readback->release(); q->release(); device->release(); pool->release();
  munmap(src,bytes);
  return 0;
}

// Exercise the two file offsets captured for the first scene with controlled bytes.
// Only the CPU-upload path copies those bytes on the CPU. Both paths wait for
// the GPU fence and compare the same contiguous readback in guest address order.
#include "d3d12_test.hpp"
#include <array>
#include <chrono>
#include <cstdint>

constexpr size_t MiB = 1024 * 1024;
struct Span { size_t logical, physical, bytes; };
using Clock = std::chrono::steady_clock;
double ms(Clock::time_point a, Clock::time_point b) {
  return std::chrono::duration<double, std::milli>(b - a).count();
}

int main() {
  unsigned ticket, section, count;
  size_t size;
  std::array<Span, 2> spans{};
  FILE *f = fopen("layout.txt", "r");
  if (!f || fscanf(f,"%u %u %zu %u",&ticket,&section,&size,&count) != 4 ||
      count != spans.size() || size != 174 * MiB) return 2;
  size_t sectionSize = 0, covered = 0;
  for (auto &s : spans) {
    if (fscanf(f,"%zu %zu %zu",&s.logical,&s.physical,&s.bytes) != 3 ||
        !s.bytes || s.logical != covered || s.physical + s.bytes > 256 * MiB) return 2;
    covered += s.bytes;
    sectionSize = std::max(sectionSize, s.physical + s.bytes);
  }
  fclose(f);
  if (covered != size) return 2;
  HANDLE file = CreateFileMappingA(INVALID_HANDLE_VALUE,nullptr,PAGE_READWRITE,0,(DWORD)sectionSize,nullptr);
  auto *raw = static_cast<uint8_t *>(MapViewOfFile(file,FILE_MAP_ALL_ACCESS,0,0,sectionSize));
  if (!raw) return 2;
  uint32_t state = 0x187187u;
  for (size_t i = 0; i < sectionSize / 4; ++i) {
    state ^= state << 13; state ^= state >> 17; state ^= state << 5;
    reinterpret_cast<uint32_t *>(raw)[i] = state;
  }
  ComPtr<ID3D12Device3> d;
  CHECK(D3D12CreateDevice(nullptr,D3D_FEATURE_LEVEL_11_0,IID_PPV_ARGS(&d)));
  D3D12_COMMAND_QUEUE_DESC qdesc{};
  ComPtr<ID3D12CommandQueue> q;
  CHECK(d->CreateCommandQueue(&qdesc,IID_PPV_ARGS(&q)));
  ComPtr<ID3D12Heap> heap;
  auto t0 = Clock::now();
  CHECK(d->OpenExistingHeapFromFileMapping(file,IID_PPV_ARGS(&heap)));
  D3D12_RESOURCE_DESC desc{D3D12_RESOURCE_DIMENSION_BUFFER,0,sectionSize,1,1,1,DXGI_FORMAT_UNKNOWN,
                           {1,0},D3D12_TEXTURE_LAYOUT_ROW_MAJOR,D3D12_RESOURCE_FLAG_ALLOW_CROSS_ADAPTER};
  ComPtr<ID3D12Resource> imported;
  CHECK(d->CreatePlacedResource(heap.Get(),0,&desc,D3D12_RESOURCE_STATE_COPY_SOURCE,nullptr,IID_PPV_ARGS(&imported)));
  printf("import_ms=%.3f size=%zu\n",ms(t0,Clock::now()),size);
  auto upload = buffer(d.Get(),D3D12_HEAP_TYPE_UPLOAD,size,D3D12_RESOURCE_STATE_GENERIC_READ);
  auto readback = buffer(d.Get(),D3D12_HEAP_TYPE_READBACK,size,D3D12_RESOURCE_STATE_COPY_DEST);
  if (!upload || !readback) return 3;
  void *staging = nullptr, *seen = nullptr;
  CHECK(upload->Map(0,nullptr,&staging)); CHECK(readback->Map(0,nullptr,&seen));
  ComPtr<ID3D12CommandAllocator> alloc;
  ComPtr<ID3D12GraphicsCommandList> list;
  CHECK(d->CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT,IID_PPV_ARGS(&alloc)));
  CHECK(d->CreateCommandList(0,D3D12_COMMAND_LIST_TYPE_DIRECT,alloc.Get(),nullptr,IID_PPV_ARGS(&list)));
  CHECK(list->Close());
  for (int round = 0; round < 10; ++round) {
    // Touch both ends of each seam on the CPU. The previous fence has signalled.
    for (const auto &s : spans) {
      raw[s.physical] ^= (uint8_t)(round + 1);
      raw[s.physical + s.bytes - 1] ^= (uint8_t)(round + 1);
    }
    for (int trial = 0; trial < 2; ++trial) {
      bool alias = (round + trial) % 2 != 0;
      auto start = Clock::now();
      CHECK(alloc->Reset()); CHECK(list->Reset(alloc.Get(),nullptr));
      if (alias) {
        for (const auto &s : spans)
          list->CopyBufferRegion(readback.Get(),s.logical,imported.Get(),s.physical,s.bytes);
      } else {
        for (const auto &s : spans)
          memcpy((uint8_t *)staging + s.logical, raw + s.physical, s.bytes);
        list->CopyBufferRegion(readback.Get(),0,upload.Get(),0,size);
      }
      CHECK(submit(d.Get(),q.Get(),list.Get()));
      auto finish = Clock::now();
      bool good = true;
      for (const auto &s : spans)
        good &= !memcmp((uint8_t *)seen + s.logical, raw + s.physical, s.bytes);
      printf("round=%d mode=%s elapsed_ms=%.3f equal=%d\n",round,alias?"import":"stage",ms(start,finish),good);
      fflush(stdout);
      if (!good) return 4;
    }
  }
  imported.Reset(); heap.Reset();
  UnmapViewOfFile(raw); CloseHandle(file);
  return 0;
}

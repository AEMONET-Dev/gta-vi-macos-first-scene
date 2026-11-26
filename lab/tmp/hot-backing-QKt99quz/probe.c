#define COBJMACROS
#include <windows.h>
#include <initguid.h>
#include <d3d12.h>
#include <stdio.h>

static void heap_result(const char *api, HRESULT hr, ID3D12Heap *heap) {
    printf("%s hr=0x%08lx heap=%s\n", api, (unsigned long)hr, heap ? "present" : "null");
    if (heap) ID3D12Heap_Release(heap);
    fflush(stdout);
}
int main(void) {
    const DWORD size = 1<<20;
    ID3D12Device *device = NULL;
    ID3D12Device3 *device3 = NULL;
    HRESULT hr = D3D12CreateDevice(NULL, D3D_FEATURE_LEVEL_12_0, &IID_ID3D12Device, (void**)&device);
    printf("device hr=0x%08lx\n", (unsigned long)hr);
    if (FAILED(hr)) return 1;
    D3D12_FEATURE_DATA_EXISTING_HEAPS feature = {0};
    hr = ID3D12Device_CheckFeatureSupport(device,D3D12_FEATURE_EXISTING_HEAPS,&feature,sizeof(feature));
    printf("existing_heaps hr=0x%08lx supported=%d\n",(unsigned long)hr,(int)feature.Supported);
    hr = ID3D12Device_QueryInterface(device, &IID_ID3D12Device3, (void**)&device3);
    printf("device3 hr=0x%08lx\n", (unsigned long)hr);
    fflush(stdout);
    if (SUCCEEDED(hr)) {
        void *addr = VirtualAlloc(NULL, size, MEM_RESERVE|MEM_COMMIT, PAGE_READWRITE);
        HANDLE mapping = CreateFileMappingW(INVALID_HANDLE_VALUE,NULL,PAGE_READWRITE|SEC_COMMIT,0,size,NULL);
        void *view = MapViewOfFile(mapping,FILE_MAP_ALL_ACCESS,0,0,size);
        if (!addr || !mapping || !view) return 2;
        memset(addr,0x35,size); memset(view,0x53,size);
        ID3D12Heap *heap = NULL;
        hr = ID3D12Device3_OpenExistingHeapFromAddress(device3,addr,&IID_ID3D12Heap,(void**)&heap);
        heap_result("private_address",hr,heap); heap = NULL;
        hr = ID3D12Device3_OpenExistingHeapFromAddress(device3,view,&IID_ID3D12Heap,(void**)&heap);
        heap_result("section_address",hr,heap); heap = NULL;
        hr = ID3D12Device3_OpenExistingHeapFromFileMapping(device3,mapping,&IID_ID3D12Heap,(void**)&heap);
        heap_result("section_handle",hr,heap);
        UnmapViewOfFile(view); CloseHandle(mapping); VirtualFree(addr,0,MEM_RELEASE);
        ID3D12Device3_Release(device3);
    }
    ID3D12Device_Release(device); return 0;
}

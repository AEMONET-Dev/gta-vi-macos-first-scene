#define COBJMACROS
#include <windows.h>
#include <d3d12.h>
#include <stdio.h>

int main(void) {
    ID3D12Device *device = NULL;
    ULONGLONG start = GetTickCount64();
    HRESULT hr = D3D12CreateDevice(NULL, D3D_FEATURE_LEVEL_12_0, &IID_ID3D12Device, (void **)&device);
    printf("D3D12CreateDevice: 0x%08lx, elapsed: %llu ms\n", (unsigned long)hr, GetTickCount64() - start);
    fflush(stdout);
    if (SUCCEEDED(hr)) {
        ID3D12Device_Release(device);
        return 0;
    }
    return 1;
}

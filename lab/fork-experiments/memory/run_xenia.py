from pathlib import Path
import subprocess
here = Path(__file__).resolve().parent
repo = here.parent/"repos/xenia-edge"
source = (repo/"src/xenia/gpu/metal/metal_shared_memory.cc").read_text()
methods=[]
for func, next_func in [("bool MetalSharedMemory::Initialize()", "void MetalSharedMemory::ClearCache"),
                        ("bool MetalSharedMemory::UploadRanges(", "bool MetalSharedMemory::InitializeTraceSubmitDownloads")]:
    methods.append(source[source.index(func):source.index(next_func)])
(here/"xenia-methods.inc").write_text("\n".join(methods))
cmd = ["clang++","-std=c++17","-O2","-fblocks","-DMTL_PRIVATE_IMPLEMENTATION","-DNS_PRIVATE_IMPLEMENTATION",
       "-I",str(repo/"third_party/metal-cpp"),"xenia_harness.cpp","-framework","Foundation","-framework","Metal",
       "-framework","QuartzCore","-o","xenia_harness"]
subprocess.run(cmd,cwd=here,check=True)
with (here/"xenia.log").open("w") as log:
    subprocess.run([str(here/"xenia_harness")],stdout=log,stderr=subprocess.STDOUT,cwd=here,check=True)

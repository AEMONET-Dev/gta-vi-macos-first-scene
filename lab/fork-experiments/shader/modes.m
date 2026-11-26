#import <Foundation/Foundation.h>
#import <Metal/Metal.h>
#include <stdio.h>
#include <math.h>
typedef struct {uint64_t first, texture, flags;} Entry;
static MTLPixelFormat pixel(int x){
 switch(x){case 10:return MTLPixelFormatRGBA16Float;case 16:return MTLPixelFormatRG32Float;case 24:return MTLPixelFormatRGB10A2Unorm;case 26:return MTLPixelFormatRG11B10Float;case 28:return MTLPixelFormatRGBA8Unorm;case 29:return MTLPixelFormatRGBA8Unorm_sRGB;case 30:return MTLPixelFormatRGBA8Uint;case 39:case 42:return MTLPixelFormatR32Uint;case 41:return MTLPixelFormatR32Float;case 54:return MTLPixelFormatR16Float;case 56:return MTLPixelFormatR16Unorm;case 61:return MTLPixelFormatR8Unorm;case 67:return MTLPixelFormatRGB9E5Float;case 83:return MTLPixelFormatBC5_RGUnorm;case 95:return MTLPixelFormatBC6H_RGBUfloat;default:return MTLPixelFormatInvalid;}
}
static MTLSamplerAddressMode address(int d){switch(d){case 1:return MTLSamplerAddressModeRepeat;case 2:return MTLSamplerAddressModeMirrorRepeat;case 3:return MTLSamplerAddressModeClampToEdge;case 4:return MTLSamplerAddressModeClampToBorderColor;case 5:return MTLSamplerAddressModeMirrorClampToEdge;default:abort();}}
static BOOL good(id<MTLCommandBuffer> cb){[cb commit];[cb waitUntilCompleted];if(cb.status!=MTLCommandBufferStatusCompleted){fprintf(stderr,"GPU error %s\n",cb.error.description.UTF8String);return NO;}return YES;}
static NSUInteger roundUp(NSUInteger v,NSUInteger a){return(v+a-1)/a*a;}
int main(int argc,const char **argv){@autoreleasepool{
if(argc!=4)return 2; NSError *error=nil;
NSDictionary *m=[NSJSONSerialization JSONObjectWithData:[NSData dataWithContentsOfFile:@(argv[1])] options:0 error:&error];
NSString *folder=m[@"directory"],*out=@(argv[3]);[[NSFileManager defaultManager]createDirectoryAtPath:out withIntermediateDirectories:YES attributes:nil error:&error];
id<MTLDevice>dev=MTLCreateSystemDefaultDevice();
MTLCompileOptions *opts=[MTLCompileOptions new]; opts.languageVersion=MTLLanguageVersion3_2;
if (getenv("REPLAY_PRECISE")) { opts.mathMode=MTLMathModeSafe; opts.mathFloatingPointFunctions=MTLMathFloatingPointFunctionsPrecise; }
opts.mathMode=(MTLMathMode)atoi(getenv("AUDIT_MODE")); opts.mathFloatingPointFunctions=(MTLMathFloatingPointFunctions)atoi(getenv("AUDIT_FN"));
NSString*source=[NSString stringWithContentsOfFile:@(argv[2]) encoding:NSUTF8StringEncoding error:&error];
id<MTLLibrary>lib=[dev newLibraryWithSource:source options:opts error:&error];if(!lib){NSLog(@"%@",error);return 3;}
id<MTLComputePipelineState>pso=[dev newComputePipelineStateWithFunction:[lib newFunctionWithName:@"main0"] error:&error];if(!pso)return 4;
id<MTLCommandQueue>q=[dev newCommandQueue];id<MTLCommandBuffer>up=[q commandBuffer];id<MTLBlitCommandEncoder>ub=[up blitCommandEncoder];
id<MTLBuffer> root=[dev newBufferWithLength:256 options:MTLResourceStorageModeShared],user=[dev newBufferWithLength:256 options:MTLResourceStorageModeShared];
NSMutableArray *keep=[NSMutableArray arrayWithObjects:root,user,nil],*useRead=[NSMutableArray arrayWithObject:user],*useRW=[NSMutableArray array];
NSUInteger maxIndex=0;for(NSDictionary*r in m[@"resources"])maxIndex=MAX(maxIndex,[r[@"index"] unsignedIntegerValue]);
id<MTLBuffer> heap=[dev newBufferWithLength:MAX(maxIndex+1,2097152ul)*8 options:MTLResourceStorageModeShared];
id<MTLBuffer> sampHeap=[dev newBufferWithLength:256 options:MTLResourceStorageModeShared], cbvHeap=[dev newBufferWithLength:256 options:MTLResourceStorageModeShared];
uint64_t*entries=heap.contents;
((uint64_t*)cbvHeap.contents)[0]=user.gpuAddress;
memset(root.contents,0,256);
for(NSString*k in m[@"users"]){((uint32_t*)user.contents)[k.intValue]=[m[@"users"][k] unsignedIntValue];}
NSUInteger i=0;for(NSDictionary*s in m[@"samplers"]){int f=[s[@"filter"]intValue]; MTLSamplerDescriptor*d=[MTLSamplerDescriptor new];d.supportArgumentBuffers=YES;d.minFilter=(f&16)?MTLSamplerMinMagFilterLinear:MTLSamplerMinMagFilterNearest;d.magFilter=(f&4)?MTLSamplerMinMagFilterLinear:MTLSamplerMinMagFilterNearest;d.mipFilter=(f&1)?MTLSamplerMipFilterLinear:MTLSamplerMipFilterNearest;
NSArray*a=s[@"address"];d.sAddressMode=address([a[0]intValue]);d.tAddressMode=address([a[1]intValue]);d.rAddressMode=address([a[2]intValue]);d.lodMinClamp=[s[@"min_lod"]floatValue];d.lodMaxClamp=[s[@"max_lod"]floatValue];d.maxAnisotropy=[s[@"max_anisotropy"]intValue]; if(f&128)d.compareFunction=(MTLCompareFunction)([s[@"comparison"]intValue]-1);
id<MTLSamplerState>sam=[dev newSamplerStateWithDescriptor:d];if(!sam){NSLog(@"sampler failed");return 5;}[keep addObject:sam];((uint64_t*)sampHeap.contents)[i++]=sam.gpuResourceID._impl;
}
NSMutableDictionary *textures=[NSMutableDictionary dictionary], *originals=[NSMutableDictionary dictionary];
for(NSDictionary*r in m[@"resources"]){@autoreleasepool{
NSString*n=r[@"name"];NSUInteger idx=[r[@"index"]unsignedIntegerValue];NSData*data=[NSData dataWithContentsOfFile:[folder stringByAppendingPathComponent:[NSString stringWithFormat:@"before-%@.bin",n]]];NSArray*w=r[@"words"];BOOL write=[r[@"kind"]isEqual:@"Uav"];
if(!r[@"levels"]){id<MTLBuffer>b=[dev newBufferWithLength:roundUp(MAX(data.length,1),256) options:MTLResourceStorageModeShared];memcpy(b.contents,data.bytes,data.length);[useRead addObject:b];[keep addObject:b];
 {MTLTextureDescriptor*td=[MTLTextureDescriptor textureBufferDescriptorWithPixelFormat:((getenv("REPLAY_TYPED_FLOAT") && [n isEqual:@"Srv-s1-d8"])?MTLPixelFormatR32Float:pixel(42)) width:data.length/4 resourceOptions:MTLResourceStorageModeShared usage:MTLTextureUsageShaderRead];id<MTLTexture>tv=[b newTextureWithDescriptor:td offset:0 bytesPerRow:data.length];if(!tv){NSLog(@"typed buffer failed %@",n);return 6;}entries[idx]=tv.gpuResourceID._impl;[useRead addObject:tv];[keep addObject:tv];}continue;}
NSUInteger width=[r[@"width"]unsignedIntegerValue],height=[r[@"height"]unsignedIntegerValue],depth=[r[@"array_or_depth"]unsignedIntegerValue],mips=[r[@"mips"]unsignedIntegerValue];bool td3=[r[@"dimension"]intValue]==4;bool bc=[w[0]intValue]==83||[w[0]intValue]==95;
MTLTextureDescriptor*d=[MTLTextureDescriptor new];d.width=width;d.height=height;d.textureType=td3 ? MTLTextureType3D : ([w[1]intValue]==2 ? MTLTextureType1D : ([w[1]intValue]==4 ? MTLTextureType2D : MTLTextureType2DArray));d.depth=td3?depth:1;d.arrayLength=td3?1:depth;d.mipmapLevelCount=mips;d.pixelFormat=pixel([w[0]intValue]);d.storageMode=MTLStorageModePrivate;d.usage=MTLTextureUsageShaderRead|(write?MTLTextureUsageShaderWrite:0);
id<MTLTexture>t=[dev newTextureWithDescriptor:d];if(!t){fprintf(stderr,"cannot make %s dxgi=%d dim=%d\n",n.UTF8String,[w[0]intValue],[r[@"dimension"]intValue]);return 7;}
t.label=n;id<MTLTexture>binding=t;
if(getenv("REPLAY_TYPED_VIEWS") && !td3 && d.arrayLength==1 && [w[1]intValue]==4) {
    binding=[t newTextureViewWithPixelFormat:d.pixelFormat textureType:MTLTextureType2D levels:NSMakeRange(0,mips) slices:NSMakeRange(0,1)];
    if(!binding)return 20;
    [(write?useRW:useRead)addObject:binding];[keep addObject:binding];
}
entries[idx]=binding.gpuResourceID._impl;[keep addObject:t];[(write?useRW:useRead)addObject:t];textures[n]=t;
NSUInteger at=0;for(NSArray*l in r[@"levels"]){NSUInteger sub=[l[0]unsignedIntegerValue],ww=[l[1]unsignedIntegerValue],hh=[l[2]unsignedIntegerValue],zz=[l[3]unsignedIntegerValue],rows=[l[4]unsignedIntegerValue],row=[l[5]unsignedIntegerValue],pitch=roundUp(row,256),size=pitch*rows*zz;
id<MTLBuffer>b=[dev newBufferWithLength:size options:MTLResourceStorageModeShared];for(NSUInteger y=0;y<rows*zz;y++)memcpy((char*)b.contents+y*pitch,(char*)data.bytes+at+y*row,row);
[ub copyFromBuffer:b sourceOffset:0 sourceBytesPerRow:pitch sourceBytesPerImage:pitch*rows sourceSize:MTLSizeMake(ww,hh,zz) toTexture:t destinationSlice:td3?0:sub/mips destinationLevel:sub%mips destinationOrigin:MTLOriginMake(0,0,0)];[keep addObject:b];at+=row*rows*zz;}
if(write){
 id<MTLTexture>snapshot=[dev newTextureWithDescriptor:d];
 [ub copyFromTexture:t toTexture:snapshot];originals[n]=snapshot;[keep addObject:snapshot];
}
if(at!=data.length){NSLog(@"invalid input length %@ %lu/%lu",n,at,data.length);return 8;}(void)bc;
}}
[ub endEncoding];if(!good(up))return 9;
NSData*args=[NSData dataWithContentsOfFile:[folder stringByAppendingPathComponent:@"arguments.bin"]];const uint32_t*groups=args.bytes;
NSUInteger repeats=MAX(1,MIN(50,[[NSProcessInfo processInfo].environment[@"REPLAY_SAMPLES"] integerValue]));
NSString *override=[NSProcessInfo processInfo].environment[@"REPLAY_GROUPS_X"];uint32_t x=override?(uint32_t)override.intValue:groups[0];
for(NSUInteger sample=0;sample<repeats;sample++){
 if(sample){id<MTLCommandBuffer>reset=[q commandBuffer];id<MTLBlitCommandEncoder>b=[reset blitCommandEncoder];for(NSString*n in originals){[b copyFromTexture:originals[n] toTexture:textures[n]];}[b endEncoding];if(!good(reset))return 12;}
id<MTLCommandBuffer>cb=[q commandBuffer];id<MTLComputeCommandEncoder>enc=[cb computeCommandEncoder];[enc setComputePipelineState:pso];[enc setBuffer:root offset:0 atIndex:6];[enc setBuffer:cbvHeap offset:0 atIndex:5];[enc setBuffer:sampHeap offset:0 atIndex:2];[enc setBuffer:heap offset:0 atIndex:0];
id<MTLBuffer>dummy=[dev newBufferWithLength:24 options:MTLResourceStorageModeShared];[enc setBuffer:dummy offset:0 atIndex:1];
for(id<MTLResource>x in useRead)[enc useResource:x usage:MTLResourceUsageRead];for(id<MTLResource>x in useRW)[enc useResource:x usage:MTLResourceUsageRead|MTLResourceUsageWrite];
[enc dispatchThreadgroups:MTLSizeMake(x,groups[1],groups[2]) threadsPerThreadgroup:MTLSizeMake(8,4,1)];[enc endEncoding];if(!good(cb))return 10;
printf("groups=%u,%u,%u gpu-start=%.9f gpu-end=%.9f compute-ms=%.4f\n",x,groups[1],groups[2],cb.GPUStartTime,cb.GPUEndTime,1000*(cb.GPUEndTime-cb.GPUStartTime));fflush(stdout);
}
for(NSDictionary*r in m[@"resources"]){if(![r[@"kind"]isEqual:@"Uav"])continue;NSString*n=r[@"name"];id<MTLTexture>t=textures[n];NSMutableData*actual=[NSMutableData data];
for(NSArray*l in r[@"levels"]){NSUInteger sub=[l[0]unsignedIntegerValue],ww=[l[1]unsignedIntegerValue],hh=[l[2]unsignedIntegerValue],zz=[l[3]unsignedIntegerValue],rows=[l[4]unsignedIntegerValue],row=[l[5]unsignedIntegerValue],pitch=roundUp(row,256);id<MTLBuffer>b=[dev newBufferWithLength:pitch*rows*zz options:MTLResourceStorageModeShared];id<MTLCommandBuffer>c=[q commandBuffer];id<MTLBlitCommandEncoder>e=[c blitCommandEncoder];[e copyFromTexture:t sourceSlice:sub/t.mipmapLevelCount sourceLevel:sub%t.mipmapLevelCount sourceOrigin:MTLOriginMake(0,0,0) sourceSize:MTLSizeMake(ww,hh,zz) toBuffer:b destinationOffset:0 destinationBytesPerRow:pitch destinationBytesPerImage:pitch*rows];[e endEncoding];if(!good(c))return 11;for(NSUInteger y=0;y<rows*zz;y++)[actual appendBytes:(char*)b.contents+y*pitch length:row];}
[actual writeToFile:[out stringByAppendingPathComponent:[n stringByAppendingString:@".bin"]] atomically:YES];}
return 0;}}

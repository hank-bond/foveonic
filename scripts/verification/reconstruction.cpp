// Replay native Merrill Stage 2 with owned storage and explicit banding presets.
// Adapted with owner permission from the reference Foveonic investigation's
// reconstruction-review.cpp. This is a measurement harness, not an application
// export, processor implementation, or proposed photographic architecture.
#include <algorithm>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <dlfcn.h>
#include <filesystem>
#include <fstream>
#include <stdexcept>
#include <string>
#include <vector>

void* symbol(void* library, const char* name) {
    void* result = dlsym(library, name);
    if (!result) throw std::runtime_error(name);
    return result;
}
template<class T> void field(void* p, size_t offset, T value) {
    std::memcpy(static_cast<unsigned char*>(p)+offset, &value, sizeof(value));
}
template<class T> T get(const void* p, size_t offset) {
    T value; std::memcpy(&value, static_cast<const unsigned char*>(p)+offset, sizeof(value)); return value;
}
struct Buffer { void* vtable; void* data; uint32_t width, height, stride, padding; };
static_assert(sizeof(Buffer)==32);
// Preserve the installed nontrivial ARM64 indirect-return convention.
struct Rect { int32_t x0,y0,x1,y1; ~Rect() {} };
struct Guarded {
    std::vector<unsigned char> bytes;
    size_t count;
    explicit Guarded(size_t n):bytes(n+128,0xa5),count(n) { std::fill(bytes.begin()+64,bytes.end()-64,0); }
    unsigned char* data() { return bytes.data()+64; }
    void check() const {
        for(size_t i=0;i<64;++i) if(bytes[i]!=0xa5 || bytes[count+64+i]!=0xa5) throw std::runtime_error("Buffer guard changed");
    }
};
void save(const std::filesystem::path& p, const void* data, size_t bytes) {
    if(std::filesystem::exists(p)) throw std::runtime_error("Output already exists");
    if(std::filesystem::space(p.parent_path()).available<uint64_t(10.25*(1ULL<<30))+bytes) throw std::runtime_error("Disk reserve");
    std::ofstream f(p,std::ios::binary); f.write(static_cast<const char*>(data),bytes);
    if(!f) throw std::runtime_error("Write failed");
}
bool cache_access(void*,void*) { return false; }
void cache_send(void*,void*) {}
extern "C" __attribute__((noinline)) void fixture_complete() { asm volatile(""); }
int main(int argc,char** argv) {
    try {
        if(argc!=9) throw std::runtime_error("Usage: worker LIBRARY CAMF PREPROCESS_F32 NEW_OUTPUT_DIR AP_NUM AP_DEN SHUTTER_NUM SHUTTER_DEN");
        const uint32_t ap_num=uint32_t(std::stoul(argv[5])),ap_den=uint32_t(std::stoul(argv[6]));
        const uint32_t sh_num=uint32_t(std::stoul(argv[7])),sh_den=uint32_t(std::stoul(argv[8]));
        if(!ap_num||!ap_den||!sh_num||!sh_den) throw std::runtime_error("Invalid capture rationals");
        const std::filesystem::path out=argv[4];
        void* lib=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL); if(!lib) throw std::runtime_error(dlerror());
        using Process=void(*)(void*,void*);
        auto process=reinterpret_cast<Process>(symbol(lib,"_ZN14FIPEngine2_F2019priProcessF20Stage2EP20FIPERequestF20Stage2"));
        const uintptr_t base=reinterpret_cast<uintptr_t>(process)-0xa87f0;
        std::ifstream f(argv[2],std::ios::binary); std::vector<unsigned char> metadata((std::istreambuf_iterator<char>(f)),{});
        if(metadata.empty()||metadata.size()>16*1024*1024) throw std::runtime_error("Invalid CAMF");
        Guarded cam(128),params(2048),adjustment(1024),wb(32),request(0x530),tiled(0x38);
        reinterpret_cast<void(*)(void*,const unsigned char*,unsigned)>(symbol(lib,"_ZN12FCamdata_F20C1EPKhj"))(cam.data(),metadata.data(),unsigned(metadata.size()));
        auto scalar=reinterpret_cast<float(*)(const char*,void*)>(symbol(lib,"_Z19GetFloatFromCamfilePKcR12FCamdata_F20"));
        const float capture_iso=scalar("CaptureISO",cam.data());
        auto rect=reinterpret_cast<Rect(*)(const char*,void*)>(symbol(lib,"_Z18GetRectFromCamfilePKcR12FCamdata_F20"));
        auto keep=rect("KeepImageArea",cam.data()),active=rect("ActiveImageArea",cam.data());
        constexpr uint32_t width=4928,height=3264;
        if(keep.x0!=0||keep.y0!=0||keep.x1!=width||keep.y1!=height) throw std::runtime_error("Unexpected native keep rectangle");
        const char* resolution=std::getenv("FOVEONIC_NOISE_RESOLUTION"); if(!resolution) resolution="HI";
        const uint32_t resolution_code=std::strcmp(resolution,"HI")==0 ? 1 : std::strcmp(resolution,"MED")==0 ? 3 : std::strcmp(resolution,"LOW")==0 ? 4 : 0;
        if(!resolution_code) throw std::runtime_error("Invalid resolution choice");
        auto control=[](const char* name) { const char* value=std::getenv(name); double v=value ? std::stod(value) : .5; if(!std::isfinite(v)||v<0||v>1) throw std::runtime_error("Invalid noise choice"); return v; };
        const double chroma=control("FOVEONIC_NOISE_CHROMA"),luminance=control("FOVEONIC_NOISE_LUMINANCE"),banding=control("FOVEONIC_NOISE_BANDING");
        const bool monochrome=std::getenv("FOVEONIC_MONOCHROME")!=nullptr;
        reinterpret_cast<void(*)(void*,const char*)>(symbol(lib,"_ZN15FUnicodeStr_F20C1EPKc"))(wb.data(),"Sunlight");
        field(adjustment.data(),0x28,uint64_t(0x0002000100010000)); field(adjustment.data(),0x38,uint32_t(std::lround(capture_iso)));
        field(adjustment.data(),0x44,ap_num); field(adjustment.data(),0x48,ap_den); field(adjustment.data(),0x4c,sh_num); field(adjustment.data(),0x50,sh_den); field(adjustment.data(),0x190,resolution_code);
        using Gather=void(*)(void*,void*,void*,double,double,double,void*,long long,bool,bool);
        reinterpret_cast<Gather>(symbol(lib,"_Z12GatherParamsRN15FPixelProc2_F2016ProcessingParamsEP12FCamdata_F20P23tagAdjustmentParametersddd15FUnicodeStr_F20xbb"))(params.data(),cam.data(),adjustment.data(),chroma,luminance,banding,wb.data(),3,false,monochrome);
        save(out/"gathered.bin",params.data(),0x230);
        constexpr size_t rgb_bytes=size_t(width)*height*12,mono_bytes=size_t(width)*height*2;
        Guarded input(rgb_bytes),output(rgb_bytes),retinex(mono_bytes),sharp(mono_bytes);
        std::ifstream raw(argv[3],std::ios::binary|std::ios::ate); if(!raw||raw.tellg()!=std::streamoff(rgb_bytes)) throw std::runtime_error("Preprocessing input size differs");
        raw.seekg(0); raw.read(reinterpret_cast<char*>(input.data()),rgb_bytes); if(!raw) throw std::runtime_error("Preprocessing input read failed");
        Buffer in{reinterpret_cast<void*>(base+0xd0c90+16),input.data(),width,height,width*12,0},rgb{in.vtable,output.data(),width,height,width*12,0};
        Buffer rtnx{reinterpret_cast<void*>(base+0xd0d18+16),retinex.data(),width,height,width*2,0},detail{rtnx.vtable,sharp.data(),width,height,width*2,0};
        const float resample=get<float>(params.data(),0x18);
        const uint32_t ew=(uint32_t(std::ceil(width*resample))+1)&~1U,eh=(uint32_t(std::ceil(height*resample))+1)&~1U;
        Guarded edge_pixels(size_t(ew)*eh*4); Buffer edge{reinterpret_cast<void*>(base+0xd0c10+16),edge_pixels.data(),ew,eh,ew*4,0};
        constexpr unsigned columns=4,rows=4,mx=64,my=64;
        constexpr size_t tile_bytes=size_t(width/columns+2*mx)*(height/rows+2*my)*12;
        Guarded tile_pixels(tile_bytes*columns*rows+std::max(size_t(ew)*eh*12,size_t(width/columns+2*mx)*12)),workspace(256*1024*1024);
        reinterpret_cast<void(*)(void*,void*,int,int,unsigned,unsigned,unsigned,unsigned)>(symbol(lib,"_ZN20FTiledPixmapFlt32RGBC1EPviijjjj"))(tiled.data(),tile_pixels.data(),width,height,columns,rows,mx,my);
        if(get<uint32_t>(tiled.data(),0x30)!=tile_bytes) throw std::runtime_error("Tile allocation mismatch");
        reinterpret_cast<void(*)(void*)>(base+0xa32b0)(request.data());
        auto assign=reinterpret_cast<void(*)(void*,const char*)>(symbol(lib,"_ZN15FUnicodeStr_F20aSEPKc"));
        assign(request.data()+0x138,"Sunlight"); assign(request.data()+0x158,"Standard"); assign(request.data()+0x190,resolution);
        std::memcpy(request.data()+0x1e0,params.data(),0x230);
        field(request.data(),0x68,&in); field(request.data(),0x70,&rgb); field(request.data(),0x78,tiled.data());
        field(request.data(),0x80,1.0); field(request.data(),0x88,1.0);
        std::memcpy(request.data()+0x90,&active,16); std::memcpy(request.data()+0xa0,&keep,16); std::memcpy(request.data()+0xb0,&keep,16);
        field(request.data(),0xe0,double(ap_num)/ap_den); field(request.data(),0xe8,cam.data()); field(request.data(),0xf8,double(std::lround(capture_iso)));
        field(request.data(),0x100,&rtnx); field(request.data(),0x108,&detail); field(request.data(),0x118,&edge);
        field(request.data(),0x410,adjustment.data()); field(request.data(),0x420,workspace.data()); field(request.data(),0x428,uint32_t(workspace.count));
        field(request.data(),0x500,&cache_access); field(request.data(),0x508,&cache_send);
        field(request.data(),0x4c4,uint8_t(monochrome));
        std::printf("banding=%g resolution=%s estimatedISO=%g scale=%g column_blend=%g window=%u level=%u\n",banding,resolution,get<float>(params.data(),4),resample,get<float>(params.data(),0xac),get<uint32_t>(params.data(),0xb0),get<uint32_t>(params.data(),0xb4)); std::fflush(stdout);
        process(nullptr,request.data()); if(get<uint32_t>(request.data(),0x10)!=2) throw std::runtime_error("Request did not complete");
        for(Guarded* b:{&cam,&params,&adjustment,&wb,&request,&tiled,&input,&output,&retinex,&sharp,&edge_pixels,&tile_pixels,&workspace}) b->check();
        auto pixels=reinterpret_cast<const float*>(output.data()); size_t nonzero=0;
        for(size_t i=0;i<rgb_bytes/4;++i) { if(!std::isfinite(pixels[i])) throw std::runtime_error("Nonfinite output"); nonzero+=pixels[i]!=0; }
        if(!nonzero) throw std::runtime_error("Empty output");
        save(out/"detail-rgb.f32",output.data(),rgb_bytes); save(out/"detail-sharp.i16",sharp.data(),mono_bytes);
        save(out/"mode-contrast.f64",request.data()+0x1d0,8);
        std::puts("status=completed finite=true guards_intact=true"); fixture_complete(); return 0;
    } catch(const std::exception& e) { std::fprintf(stderr,"%s\n",e.what()); return 1; }
}

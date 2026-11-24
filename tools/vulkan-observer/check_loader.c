/* Test loader negotiation with libvulkan already loaded. No GPU calls. */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <stdio.h>
#include <vulkan/vk_layer.h>
#define CHECK(x) do { if (!(x)) { fprintf(stderr, "failed: %s\n", #x); return 1; } } while (0)
int main(int argc, char **argv) {
    CHECK(argc == 2);
    void *library = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL);
    CHECK(library != NULL);
    PFN_vkNegotiateLoaderLayerInterfaceVersion negotiate =
        (PFN_vkNegotiateLoaderLayerInterfaceVersion)dlsym(library, "vkNegotiateLoaderLayerInterfaceVersion");
    CHECK(negotiate != NULL);
    CHECK(negotiate(NULL) == VK_ERROR_INITIALIZATION_FAILED);
    VkNegotiateLayerInterface info = {0};
    info.sType = LAYER_NEGOTIATE_INTERFACE_STRUCT;
    info.loaderLayerInterfaceVersion = 1;
    CHECK(negotiate(&info) == VK_ERROR_INITIALIZATION_FAILED);
    info.loaderLayerInterfaceVersion = 3;
    CHECK(negotiate(&info) == VK_SUCCESS);
    CHECK(info.loaderLayerInterfaceVersion == 2);
    CHECK(info.pfnGetInstanceProcAddr && info.pfnGetDeviceProcAddr && info.pfnGetPhysicalDeviceProcAddr);
    /* This comparison keeps libvulkan in the process and detects preemption. */
    CHECK(info.pfnGetInstanceProcAddr != vkGetInstanceProcAddr);
    CHECK(info.pfnGetDeviceProcAddr != vkGetDeviceProcAddr);
    Dl_info exported = {0}, instance = {0}, device = {0};
    CHECK(dladdr((void *)negotiate, &exported));
    CHECK(dladdr((void *)info.pfnGetInstanceProcAddr, &instance));
    CHECK(dladdr((void *)info.pfnGetDeviceProcAddr, &device));
    CHECK(exported.dli_fbase == instance.dli_fbase && exported.dli_fbase == device.dli_fbase);
    CHECK(info.pfnGetInstanceProcAddr(VK_NULL_HANDLE, "vkCreateInstance") != NULL);
    CHECK(info.pfnGetInstanceProcAddr(VK_NULL_HANDLE, "vkNotAFunction") == NULL);
    CHECK(info.pfnGetDeviceProcAddr(VK_NULL_HANDLE, "vkCmdDispatch") == NULL);
    CHECK(info.pfnGetPhysicalDeviceProcAddr(VK_NULL_HANDLE, "vkNotAFunction") == NULL);
    CHECK(dlclose(library) == 0);
    puts("loader negotiation and symbol ownership passed");
    return 0;
}

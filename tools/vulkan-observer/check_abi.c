/* Check the hand-written Rust loader layout against the installed C header. */
#include <stddef.h>
#include <vulkan/vk_layer.h>
_Static_assert(sizeof(void *) == 8, "64-bit only");
_Static_assert(sizeof(VkNegotiateLayerInterface) == 48, "negotiation size");
_Static_assert(offsetof(VkNegotiateLayerInterface, pfnGetInstanceProcAddr) == 24, "GIPA offset");
_Static_assert(sizeof(VkLayerInstanceCreateInfo) == 40, "instance create size");
_Static_assert(offsetof(VkLayerInstanceCreateInfo, u) == 24, "instance union offset");
_Static_assert(sizeof(VkLayerDeviceCreateInfo) == 32, "device create size");
_Static_assert(offsetof(VkLayerDeviceCreateInfo, u) == 24, "device union offset");
_Static_assert(sizeof(VkLayerInstanceLink) == 24, "instance link size");
_Static_assert(sizeof(VkLayerDeviceLink) == 24, "device link size");
_Static_assert(LAYER_NEGOTIATE_INTERFACE_STRUCT == 1, "negotiation type");
_Static_assert(VK_LAYER_LINK_INFO == 0, "link info type");
int main(void) { return 0; }

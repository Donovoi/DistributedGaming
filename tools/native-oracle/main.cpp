#include <vulkan/vulkan.h>
#include <openssl/sha.h>
#include <algorithm>
#include <array>
#include <atomic>
#include <cstring>
#include <cstdlib>
#include <fstream>
#include <iomanip>
#include <iostream>
#include <sstream>
#include <stdexcept>
#include <string>
#include <vector>

// Fixed shader validation tool. Not a game hook or a general shader runner.
static std::atomic<unsigned> errors{0};
static void check(VkResult r) {
    if (r != VK_SUCCESS) throw std::runtime_error("Vulkan result " + std::to_string(r));
}
static VKAPI_ATTR VkBool32 VKAPI_CALL message(VkDebugUtilsMessageSeverityFlagBitsEXT severity,
        VkDebugUtilsMessageTypeFlagsEXT, const VkDebugUtilsMessengerCallbackDataEXT* data, void*) {
    if (severity & VK_DEBUG_UTILS_MESSAGE_SEVERITY_ERROR_BIT_EXT) ++errors;
    std::cerr << data->pMessage << '\n';
    return VK_FALSE;
}
static std::vector<unsigned char> read(const char* name, size_t size, bool exact=true) {
    std::ifstream f(name, std::ios::binary);
    if (!f) throw std::runtime_error("Missing input");
    std::vector<unsigned char> b(size+1);
    f.read(reinterpret_cast<char*>(b.data()), static_cast<std::streamsize>(b.size()));
    b.resize(static_cast<size_t>(f.gcount()));
    if ((exact && b.size()!=size) || (!exact && (b.empty() || b.size()>size)))
        throw std::runtime_error("Wrong input size");
    return b;
}
static std::string hash(const std::vector<unsigned char>& b) {
    std::array<unsigned char,SHA256_DIGEST_LENGTH> digest{};
    SHA256(b.data(),b.size(),digest.data());
    std::ostringstream out;
    for(auto c:digest) out << std::hex << std::setw(2) << std::setfill('0') << unsigned(c);
    return out.str();
}
struct Buffer { VkBuffer buffer{}; VkDeviceMemory memory{}; unsigned char* mapped{}; size_t size{}; };
struct Context {
    VkInstance instance{}; VkDebugUtilsMessengerEXT debug{};
    VkPhysicalDevice physical{}; VkPhysicalDeviceProperties props{};
    VkDevice device{}; VkQueue queue{}; uint32_t family{};
    VkDescriptorSetLayout layout{}; VkPipelineLayout pipeline_layout{};
    VkShaderModule shader{}; VkPipeline pipeline{}; VkDescriptorPool descriptors{};
    VkCommandPool commands{}; VkFence fence{}; bool in_flight=false;
    std::vector<Buffer> buffers; std::vector<VkBufferView> views;
    ~Context() { close(); }
    void close() {
        if(device) {
            // A failed completion must not enter an unbounded wait during cleanup.
            if(in_flight) {std::cerr << "Submission did not complete; exiting without device cleanup\n";std::_Exit(2);}
            if(fence) vkDestroyFence(device,fence,nullptr);
            if(commands) vkDestroyCommandPool(device,commands,nullptr);
            if(pipeline) vkDestroyPipeline(device,pipeline,nullptr);
            if(shader) vkDestroyShaderModule(device,shader,nullptr);
            if(descriptors) vkDestroyDescriptorPool(device,descriptors,nullptr);
            if(pipeline_layout) vkDestroyPipelineLayout(device,pipeline_layout,nullptr);
            if(layout) vkDestroyDescriptorSetLayout(device,layout,nullptr);
            for(auto v:views) vkDestroyBufferView(device,v,nullptr);
            for(auto b:buffers) {
                if(b.mapped) vkUnmapMemory(device,b.memory);
                if(b.buffer) vkDestroyBuffer(device,b.buffer,nullptr);
                if(b.memory) vkFreeMemory(device,b.memory,nullptr);
            }
            vkDestroyDevice(device,nullptr);
            device=VK_NULL_HANDLE;
        }
        if(debug) {
            auto destroy=reinterpret_cast<PFN_vkDestroyDebugUtilsMessengerEXT>(
                vkGetInstanceProcAddr(instance,"vkDestroyDebugUtilsMessengerEXT"));
            if(destroy) destroy(instance,debug,nullptr);
            debug=VK_NULL_HANDLE;
        }
        if(instance) {vkDestroyInstance(instance,nullptr);instance=VK_NULL_HANDLE;}
    }
    Buffer make_buffer(size_t size,VkBufferUsageFlags usage) {
        buffers.emplace_back(); auto& b=buffers.back(); b.size=size;
        VkBufferCreateInfo info{VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO};
        info.size=size; info.usage=usage; info.sharingMode=VK_SHARING_MODE_EXCLUSIVE;
        check(vkCreateBuffer(device,&info,nullptr,&b.buffer));
        VkMemoryRequirements req{}; vkGetBufferMemoryRequirements(device,b.buffer,&req);
        VkPhysicalDeviceMemoryProperties mem{}; vkGetPhysicalDeviceMemoryProperties(physical,&mem);
        uint32_t type=mem.memoryTypeCount;
        for(uint32_t i=0;i<mem.memoryTypeCount;++i)
            if((req.memoryTypeBits&(1u<<i)) &&
               (mem.memoryTypes[i].propertyFlags&(VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT|VK_MEMORY_PROPERTY_HOST_COHERENT_BIT))==
               (VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT|VK_MEMORY_PROPERTY_HOST_COHERENT_BIT)) {type=i;break;}
        if(type==mem.memoryTypeCount) throw std::runtime_error("No coherent host-visible memory");
        VkMemoryAllocateInfo alloc{VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO};
        alloc.allocationSize=req.size; alloc.memoryTypeIndex=type;
        check(vkAllocateMemory(device,&alloc,nullptr,&b.memory));
        check(vkBindBufferMemory(device,b.buffer,b.memory,0));
        void* ptr=nullptr; check(vkMapMemory(device,b.memory,0,size,0,&ptr));
        b.mapped=static_cast<unsigned char*>(ptr); return b;
    }
    VkBufferView view(Buffer b,VkFormat format,VkDeviceSize offset,VkDeviceSize range) {
        VkBufferViewCreateInfo info{VK_STRUCTURE_TYPE_BUFFER_VIEW_CREATE_INFO};
        info.buffer=b.buffer; info.format=format; info.offset=offset; info.range=range;
        VkBufferView v{}; check(vkCreateBufferView(device,&info,nullptr,&v)); views.push_back(v); return v;
    }
};
int main(int argc,char** argv) {
    try {
        if(argc==2 && std::string(argv[1])=="--help") {
            std::cout << "native-oracle SHADER HISTOGRAM UNIFORM\nFixed CS2 shader only. Run on a remote NVIDIA Vulkan GPU.\nUse an external 45-second process timeout. This tool does not prove game speedup.\n";
            return 0;
        }
        if(argc!=4) throw std::runtime_error("Use: native_reduction SHADER HISTOGRAM UNIFORM");
        const auto code=read(argv[1],1024*1024,false);
        const auto histogram=read(argv[2],1024), uniform=read(argv[3],32);
        const std::string expected="5b112e9919dff8911604f606a636b324e197cde9797196efd604b5f9ca48f9c7";
        if(hash(code)!=expected || code.size()%4) throw std::runtime_error("Shader identity mismatch");
        std::vector<uint32_t> words(code.size()/4); std::memcpy(words.data(),code.data(),code.size());
        Context c;
        const char* layer="VK_LAYER_KHRONOS_validation";
        const char* extensions[]={VK_EXT_DEBUG_UTILS_EXTENSION_NAME,VK_EXT_VALIDATION_FEATURES_EXTENSION_NAME};
        VkApplicationInfo app{VK_STRUCTURE_TYPE_APPLICATION_INFO}; app.pApplicationName="DG native oracle"; app.apiVersion=VK_API_VERSION_1_2;
        VkDebugUtilsMessengerCreateInfoEXT debug{VK_STRUCTURE_TYPE_DEBUG_UTILS_MESSENGER_CREATE_INFO_EXT};
        debug.messageSeverity=VK_DEBUG_UTILS_MESSAGE_SEVERITY_ERROR_BIT_EXT|VK_DEBUG_UTILS_MESSAGE_SEVERITY_WARNING_BIT_EXT;
        debug.messageType=VK_DEBUG_UTILS_MESSAGE_TYPE_GENERAL_BIT_EXT|VK_DEBUG_UTILS_MESSAGE_TYPE_VALIDATION_BIT_EXT|VK_DEBUG_UTILS_MESSAGE_TYPE_PERFORMANCE_BIT_EXT;
        debug.pfnUserCallback=message;
        VkValidationFeatureEnableEXT sync=VK_VALIDATION_FEATURE_ENABLE_SYNCHRONIZATION_VALIDATION_EXT;
        VkValidationFeaturesEXT validation{VK_STRUCTURE_TYPE_VALIDATION_FEATURES_EXT};
        validation.enabledValidationFeatureCount=1; validation.pEnabledValidationFeatures=&sync; validation.pNext=&debug;
        VkInstanceCreateInfo instance{VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO}; instance.pApplicationInfo=&app;
        instance.enabledLayerCount=1; instance.ppEnabledLayerNames=&layer;
        instance.enabledExtensionCount=2; instance.ppEnabledExtensionNames=extensions; instance.pNext=&validation;
        check(vkCreateInstance(&instance,nullptr,&c.instance));
        auto create_debug=reinterpret_cast<PFN_vkCreateDebugUtilsMessengerEXT>(vkGetInstanceProcAddr(c.instance,"vkCreateDebugUtilsMessengerEXT"));
        if(!create_debug) throw std::runtime_error("No debug callback");
        check(create_debug(c.instance,&debug,nullptr,&c.debug));
        uint32_t count=0; check(vkEnumeratePhysicalDevices(c.instance,&count,nullptr));
        std::vector<VkPhysicalDevice> devices(count); check(vkEnumeratePhysicalDevices(c.instance,&count,devices.data()));
        for(auto p:devices) {
            VkPhysicalDeviceProperties props{}; vkGetPhysicalDeviceProperties(p,&props);
            if(props.vendorID==0x10de && props.deviceType==VK_PHYSICAL_DEVICE_TYPE_DISCRETE_GPU && props.apiVersion>=VK_API_VERSION_1_2) {
                c.physical=p;c.props=props;break;
            }
        }
        if(!c.physical) throw std::runtime_error("No NVIDIA Vulkan 1.2 GPU");
        if(c.props.limits.maxComputeWorkGroupInvocations<256 || c.props.limits.maxComputeWorkGroupSize[0]<256 ||
           c.props.limits.maxComputeSharedMemorySize<1024 || c.props.limits.maxTexelBufferElements<256 ||
           c.props.limits.maxUniformBufferRange<32) throw std::runtime_error("Insufficient device limits");
        for(auto format:{VK_FORMAT_R32_UINT,VK_FORMAT_R32_SFLOAT}) {
            VkFormatProperties props{}; vkGetPhysicalDeviceFormatProperties(c.physical,format,&props);
            if(!(props.bufferFeatures&VK_FORMAT_FEATURE_STORAGE_TEXEL_BUFFER_BIT)) throw std::runtime_error("Unsupported texel format");
        }
        uint32_t families=0; vkGetPhysicalDeviceQueueFamilyProperties(c.physical,&families,nullptr);
        std::vector<VkQueueFamilyProperties> queues(families); vkGetPhysicalDeviceQueueFamilyProperties(c.physical,&families,queues.data());
        c.family=families;
        for(uint32_t i=0;i<families;++i) if(queues[i].queueCount && (queues[i].queueFlags&VK_QUEUE_COMPUTE_BIT)) {c.family=i;break;}
        if(c.family==families) throw std::runtime_error("No compute queue");
        float priority=1.0f;
        VkDeviceQueueCreateInfo q{VK_STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO}; q.queueFamilyIndex=c.family;q.queueCount=1;q.pQueuePriorities=&priority;
        VkDeviceCreateInfo device{VK_STRUCTURE_TYPE_DEVICE_CREATE_INFO}; device.queueCreateInfoCount=1;device.pQueueCreateInfos=&q;
        check(vkCreateDevice(c.physical,&device,nullptr,&c.device));vkGetDeviceQueue(c.device,c.family,0,&c.queue);
        const size_t offset=std::max<VkDeviceSize>(4,c.props.limits.minTexelBufferOffsetAlignment);
        Buffer hist=c.make_buffer(1088,VK_BUFFER_USAGE_STORAGE_TEXEL_BUFFER_BIT);
        Buffer params=c.make_buffer(96,VK_BUFFER_USAGE_UNIFORM_BUFFER_BIT);
        Buffer output=c.make_buffer(offset+4+64,VK_BUFFER_USAGE_STORAGE_TEXEL_BUFFER_BIT);
        const auto hview=c.view(hist,VK_FORMAT_R32_UINT,0,1024), oview=c.view(output,VK_FORMAT_R32_SFLOAT,offset,4);
        const std::array<VkDescriptorSetLayoutBinding,3> bindings{{
            {2,VK_DESCRIPTOR_TYPE_UNIFORM_BUFFER,1,VK_SHADER_STAGE_COMPUTE_BIT,nullptr},
            {159,VK_DESCRIPTOR_TYPE_STORAGE_TEXEL_BUFFER,1,VK_SHADER_STAGE_COMPUTE_BIT,nullptr},
            {160,VK_DESCRIPTOR_TYPE_STORAGE_TEXEL_BUFFER,1,VK_SHADER_STAGE_COMPUTE_BIT,nullptr}}};
        VkDescriptorSetLayoutCreateInfo layout{VK_STRUCTURE_TYPE_DESCRIPTOR_SET_LAYOUT_CREATE_INFO}; layout.bindingCount=3;layout.pBindings=bindings.data();
        check(vkCreateDescriptorSetLayout(c.device,&layout,nullptr,&c.layout));
        VkPipelineLayoutCreateInfo pl{VK_STRUCTURE_TYPE_PIPELINE_LAYOUT_CREATE_INFO};pl.setLayoutCount=1;pl.pSetLayouts=&c.layout;
        check(vkCreatePipelineLayout(c.device,&pl,nullptr,&c.pipeline_layout));
        VkShaderModuleCreateInfo module{VK_STRUCTURE_TYPE_SHADER_MODULE_CREATE_INFO};module.codeSize=code.size();module.pCode=words.data();
        check(vkCreateShaderModule(c.device,&module,nullptr,&c.shader));
        VkComputePipelineCreateInfo pipeline{VK_STRUCTURE_TYPE_COMPUTE_PIPELINE_CREATE_INFO};pipeline.layout=c.pipeline_layout;
        pipeline.stage.sType=VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO;pipeline.stage.stage=VK_SHADER_STAGE_COMPUTE_BIT;
        pipeline.stage.module=c.shader;pipeline.stage.pName="MainCs";
        check(vkCreateComputePipelines(c.device,VK_NULL_HANDLE,1,&pipeline,nullptr,&c.pipeline));
        const std::array<VkDescriptorPoolSize,2> sizes{{{VK_DESCRIPTOR_TYPE_UNIFORM_BUFFER,1},{VK_DESCRIPTOR_TYPE_STORAGE_TEXEL_BUFFER,2}}};
        VkDescriptorPoolCreateInfo pool{VK_STRUCTURE_TYPE_DESCRIPTOR_POOL_CREATE_INFO};pool.maxSets=1;pool.poolSizeCount=2;pool.pPoolSizes=sizes.data();
        check(vkCreateDescriptorPool(c.device,&pool,nullptr,&c.descriptors));
        VkDescriptorSetAllocateInfo sa{VK_STRUCTURE_TYPE_DESCRIPTOR_SET_ALLOCATE_INFO};sa.descriptorPool=c.descriptors;sa.descriptorSetCount=1;sa.pSetLayouts=&c.layout;
        VkDescriptorSet set{};check(vkAllocateDescriptorSets(c.device,&sa,&set));
        VkDescriptorBufferInfo uniform_info{params.buffer,0,32};
        std::array<VkWriteDescriptorSet,3> writes{};
        for(size_t i=0;i<3;++i) {writes[i].sType=VK_STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET;writes[i].dstSet=set;writes[i].dstBinding=bindings[i].binding;writes[i].descriptorCount=1;writes[i].descriptorType=bindings[i].descriptorType;}
        writes[0].pBufferInfo=&uniform_info;writes[1].pTexelBufferView=&hview;writes[2].pTexelBufferView=&oview;
        vkUpdateDescriptorSets(c.device,3,writes.data(),0,nullptr);
        VkCommandPoolCreateInfo cp{VK_STRUCTURE_TYPE_COMMAND_POOL_CREATE_INFO};cp.queueFamilyIndex=c.family;cp.flags=VK_COMMAND_POOL_CREATE_RESET_COMMAND_BUFFER_BIT;
        check(vkCreateCommandPool(c.device,&cp,nullptr,&c.commands));
        VkCommandBufferAllocateInfo ca{VK_STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO};ca.commandPool=c.commands;ca.level=VK_COMMAND_BUFFER_LEVEL_PRIMARY;ca.commandBufferCount=1;
        VkCommandBuffer command{};check(vkAllocateCommandBuffers(c.device,&ca,&command));
        VkFenceCreateInfo fc{VK_STRUCTURE_TYPE_FENCE_CREATE_INFO};check(vkCreateFence(c.device,&fc,nullptr,&c.fence));
        std::vector<unsigned char> hist_initial(hist.size,0xa5), uniform_initial(params.size,0xa5);
        std::copy(histogram.begin(),histogram.end(),hist_initial.begin());std::copy(uniform.begin(),uniform.end(),uniform_initial.begin());
        std::ostringstream reports; bool all=true; unsigned run=0; uint32_t first_bits=0;
        for(uint32_t sentinel:{0u,0xffffffffu,0x12345678u}) {
            std::memcpy(hist.mapped,hist_initial.data(),hist.size);std::memcpy(params.mapped,uniform_initial.data(),params.size);
            std::vector<unsigned char> initial(output.size,0xcd);std::memcpy(initial.data()+offset,&sentinel,4);std::memcpy(output.mapped,initial.data(),output.size);
            check(vkResetCommandBuffer(command,0));check(vkResetFences(c.device,1,&c.fence));
            VkCommandBufferBeginInfo begin{VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO};begin.flags=VK_COMMAND_BUFFER_USAGE_ONE_TIME_SUBMIT_BIT;
            check(vkBeginCommandBuffer(command,&begin));
            VkMemoryBarrier upload{VK_STRUCTURE_TYPE_MEMORY_BARRIER};upload.srcAccessMask=VK_ACCESS_HOST_WRITE_BIT;upload.dstAccessMask=VK_ACCESS_SHADER_READ_BIT|VK_ACCESS_SHADER_WRITE_BIT|VK_ACCESS_UNIFORM_READ_BIT;
            vkCmdPipelineBarrier(command,VK_PIPELINE_STAGE_HOST_BIT,VK_PIPELINE_STAGE_COMPUTE_SHADER_BIT,0,1,&upload,0,nullptr,0,nullptr);
            vkCmdBindPipeline(command,VK_PIPELINE_BIND_POINT_COMPUTE,c.pipeline);vkCmdBindDescriptorSets(command,VK_PIPELINE_BIND_POINT_COMPUTE,c.pipeline_layout,0,1,&set,0,nullptr);
            vkCmdDispatch(command,1,1,1);
            VkMemoryBarrier download{VK_STRUCTURE_TYPE_MEMORY_BARRIER};download.srcAccessMask=VK_ACCESS_SHADER_WRITE_BIT;download.dstAccessMask=VK_ACCESS_HOST_READ_BIT;
            vkCmdPipelineBarrier(command,VK_PIPELINE_STAGE_COMPUTE_SHADER_BIT,VK_PIPELINE_STAGE_HOST_BIT,0,1,&download,0,nullptr,0,nullptr);
            check(vkEndCommandBuffer(command));
            VkSubmitInfo submit{VK_STRUCTURE_TYPE_SUBMIT_INFO};submit.commandBufferCount=1;submit.pCommandBuffers=&command;
            c.in_flight=true;
            check(vkQueueSubmit(c.queue,1,&submit,c.fence));check(vkWaitForFences(c.device,1,&c.fence,VK_TRUE,5'000'000'000ull));
            c.in_flight=false;
            uint32_t bits=0;std::memcpy(&bits,output.mapped+offset,4);
            const bool inputs=std::memcmp(hist.mapped,hist_initial.data(),hist.size)==0 && std::memcmp(params.mapped,uniform_initial.data(),params.size)==0;
            const bool guards=std::memcmp(output.mapped,initial.data(),offset)==0 && std::memcmp(output.mapped+offset+4,initial.data()+offset+4,64)==0;
            if(run==0) first_bits=bits;
            all=all&&inputs&&guards&&(bits==first_bits);
            if(run++) reports << ',';
            reports << "{\"sentinel\":" << sentinel << ",\"output_bits\":" << bits << ",\"inputs_unchanged\":" << (inputs?"true":"false") << ",\"guards_unchanged\":" << (guards?"true":"false") << '}';
        }
        // Include errors emitted during teardown in the final decision.
        c.close();
        std::cout << "{\"shader_sha256\":\"" << expected << "\",\"histogram_sha256\":\"" << hash(histogram) << "\",\"uniform_sha256\":\"" << hash(uniform)
                  << "\",\"adapter\":\"" << c.props.deviceName << "\",\"driver_version_raw\":" << c.props.driverVersion << ",\"api_version_raw\":" << c.props.apiVersion
                  << ",\"validation_errors\":" << errors.load() << ",\"synchronization_validation\":true,\"output_view_offset\":" << offset
                  << ",\"original_shader_executed\":true,\"runs\":[" << reports.str() << "]}\n";
        if(!all || errors.load()) throw std::runtime_error("Output, guard, input or validation check failed");
    } catch(const std::exception& e) {std::cerr << e.what() << '\n';return 1;}
}

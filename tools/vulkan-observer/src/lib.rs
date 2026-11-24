//! Process-scoped Vulkan observation. No game work is replaced.
//! FFI pointers follow the Vulkan valid-call and loader-chain contracts.
#![cfg(target_pointer_width = "64")]
mod abi;
mod journal;
mod track;

use ash::vk::{self, Handle};
use journal::Journal;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    ffi::{c_char, c_void, CStr},
    fs::{File, OpenOptions},
    panic::{catch_unwind, AssertUnwindSafe},
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, MutexGuard, OnceLock,
    },
};

const MAX_ARRAY: usize = 4096;
const MAX_SHADER_BYTES: usize = 1024 * 1024;
static PANICKED: AtomicBool = AtomicBool::new(false);

struct Observation {
    tracker: track::Tracker,
    journal: Option<Journal<File>>,
    instances: u64,
    missing_log: bool,
}
impl Observation {
    fn new() -> Self {
        let file = std::env::var_os("DG_OBSERVER_LOG").and_then(|p| {
            let path = Path::new(&p);
            path.is_absolute()
                .then(|| {
                    OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(path)
                        .ok()
                })
                .flatten()
        });
        let missing_log = file.is_none();
        let mut result = Self {
            tracker: track::Tracker::default(),
            journal: file.map(|f| Journal::new(f, 8 * 1024 * 1024, 4096)),
            instances: 0,
            missing_log,
        };
        result.emit(
            json!({"event":"observer_start","schema":1,"pid":std::process::id(),
            "target_shader_sha256":track::TARGET,"observation_only":true,
            "descriptor_contents_tracked":false,"memory_ownership_tracked":false}),
        );
        result
    }
    fn emit(&mut self, value: Value) {
        if let Some(j) = &mut self.journal {
            j.write(&value);
        }
    }
    fn maybe(&mut self, value: Option<Value>) {
        if let Some(v) = value {
            self.emit(v);
        }
    }
    fn end_instance(&mut self) {
        self.instances = self.instances.saturating_sub(1);
        let dropped = self.journal.as_ref().map_or(0, |j| j.dropped);
        let io_failed = self.missing_log || self.journal.as_ref().is_some_and(|j| j.failed);
        let summary = json!({"event":"instance_lifetime_end","open_instances":self.instances,
            "recorded_target_dispatches":self.tracker.recorded,"submitted_target_dispatches":self.tracker.submitted,
            "unsupported_observations":self.tracker.unsupported,"observation_disabled":self.tracker.observation_disabled,
            "extension_function_queries":self.tracker.extension_queries,
            "dropped_records":dropped,"log_io_failed":io_failed,"observation_panicked":PANICKED.load(Ordering::Relaxed),
            "execution_proven":false,"resource_contract_complete":false});
        if let Some(j) = &mut self.journal {
            j.footer(&summary);
        }
    }
}
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}
fn observe(f: impl FnOnce(&mut Observation)) {
    static OBS: OnceLock<Mutex<Observation>> = OnceLock::new();
    if catch_unwind(AssertUnwindSafe(|| {
        f(&mut lock(
            OBS.get_or_init(|| Mutex::new(Observation::new())),
        ))
    }))
    .is_err()
    {
        PANICKED.store(true, Ordering::Relaxed);
    }
}

struct InstanceDispatch {
    handle: vk::Instance,
    gpa: vk::PFN_vkGetInstanceProcAddr,
    physical: Option<abi::PhysicalGpa>,
    fns: ash::InstanceFnV1_0,
}
struct DeviceDispatch {
    handle: vk::Device,
    gpa: vk::PFN_vkGetDeviceProcAddr,
    fns: ash::DeviceFnV1_0,
}
#[derive(Default)]
struct Dispatch {
    instances: HashMap<usize, Arc<InstanceDispatch>>,
    devices: HashMap<usize, Arc<DeviceDispatch>>,
}
fn dispatch() -> &'static Mutex<Dispatch> {
    static D: OnceLock<Mutex<Dispatch>> = OnceLock::new();
    D.get_or_init(|| Mutex::new(Dispatch::default()))
}

// Dispatchable Vulkan handles begin with the loader/layer dispatch pointer.
// This is read only for valid non-null handles supplied by Vulkan or the loader.
unsafe fn key(handle: u64) -> usize {
    if handle == 0 {
        0
    } else {
        unsafe { *(handle as *const usize) }
    }
}
unsafe fn instance(handle: u64) -> Arc<InstanceDispatch> {
    let k = unsafe { key(handle) };
    lock(dispatch())
        .instances
        .get(&k)
        .cloned()
        .unwrap_or_else(|| std::process::abort())
}
unsafe fn device(handle: u64) -> Arc<DeviceDispatch> {
    let k = unsafe { key(handle) };
    lock(dispatch())
        .devices
        .get(&k)
        .cloned()
        .unwrap_or_else(|| std::process::abort())
}
// Observation limits never alter the count or pointer passed downstream.
unsafe fn array<'a, T>(ptr: *const T, count: usize) -> Option<&'a [T]> {
    if count == 0 {
        Some(&[])
    } else if ptr.is_null() || count > MAX_ARRAY {
        None
    } else {
        Some(unsafe { std::slice::from_raw_parts(ptr, count) })
    }
}
fn unknown() {
    observe(|o| o.tracker.unknown());
}

/// # Safety
/// Called by a Vulkan loader with its writable negotiation structure.
#[no_mangle]
pub unsafe extern "system" fn vkNegotiateLoaderLayerInterfaceVersion(
    info: *mut abi::Negotiate,
) -> vk::Result {
    if info.is_null() {
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    }
    let i = unsafe { &mut *info };
    if i.s_type != 1 || i.version < 2 {
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    }
    i.version = 2;
    i.instance_gpa = Some(instance_gpa);
    i.device_gpa = Some(device_gpa);
    i.physical_gpa = Some(physical_gpa);
    vk::Result::SUCCESS
}

unsafe extern "system" fn create_instance(
    info: *const vk::InstanceCreateInfo<'_>,
    alloc: *const vk::AllocationCallbacks<'_>,
    out: *mut vk::Instance,
) -> vk::Result {
    if info.is_null() || out.is_null() {
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    }
    let Some(chain) = (unsafe { abi::instance_link((*info).p_next) }) else {
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    };
    let link = unsafe { (*chain).data.link };
    if link.is_null() {
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    }
    let gpa = unsafe { (*link).instance_gpa };
    let physical = unsafe { (*link).physical_gpa };
    let Some(raw) = (unsafe { gpa(vk::Instance::null(), c"vkCreateInstance".as_ptr()) }) else {
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    };
    let next: vk::PFN_vkCreateInstance = unsafe { std::mem::transmute(raw) };
    unsafe {
        (*chain).data.link = (*link).next;
    }
    let result = unsafe { next(info, alloc, out) };
    if result == vk::Result::SUCCESS {
        let handle = unsafe { *out };
        let fns = ash::InstanceFnV1_0::load(|name| unsafe {
            gpa(handle, name.as_ptr()).map_or(std::ptr::null(), |f| f as *const c_void)
        });
        lock(dispatch()).instances.insert(
            unsafe { key(handle.as_raw()) },
            Arc::new(InstanceDispatch {
                handle,
                gpa,
                physical,
                fns,
            }),
        );
        observe(|o| {
            o.instances = o.instances.saturating_add(1);
            o.emit(json!({"event":"instance_created"}));
        });
    }
    result
}
unsafe extern "system" fn destroy_instance(
    handle: vk::Instance,
    alloc: *const vk::AllocationCallbacks<'_>,
) {
    if handle == vk::Instance::null() {
        return;
    }
    let d = unsafe { instance(handle.as_raw()) };
    let k = unsafe { key(handle.as_raw()) };
    unsafe {
        (d.fns.destroy_instance)(handle, alloc);
    }
    let mut state = lock(dispatch());
    if state
        .instances
        .get(&k)
        .is_some_and(|current| Arc::ptr_eq(current, &d))
    {
        state.instances.remove(&k);
    }
    drop(state);
    observe(Observation::end_instance);
}
unsafe extern "system" fn create_device(
    physical: vk::PhysicalDevice,
    info: *const vk::DeviceCreateInfo<'_>,
    alloc: *const vk::AllocationCallbacks<'_>,
    out: *mut vk::Device,
) -> vk::Result {
    if info.is_null() || out.is_null() {
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    }
    let parent = unsafe { instance(physical.as_raw()) };
    let Some(chain) = (unsafe { abi::device_link((*info).p_next) }) else {
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    };
    let link = unsafe { (*chain).data.link };
    if link.is_null() {
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    }
    let igpa = unsafe { (*link).instance_gpa };
    let gpa = unsafe { (*link).device_gpa };
    let Some(raw) = (unsafe { igpa(parent.handle, c"vkCreateDevice".as_ptr()) }) else {
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    };
    let next: vk::PFN_vkCreateDevice = unsafe { std::mem::transmute(raw) };
    unsafe {
        (*chain).data.link = (*link).next;
    }
    let result = unsafe { next(physical, info, alloc, out) };
    if result == vk::Result::SUCCESS {
        let handle = unsafe { *out };
        let fns = ash::DeviceFnV1_0::load(|name| unsafe {
            gpa(handle, name.as_ptr()).map_or(std::ptr::null(), |f| f as *const c_void)
        });
        lock(dispatch()).devices.insert(
            unsafe { key(handle.as_raw()) },
            Arc::new(DeviceDispatch { handle, gpa, fns }),
        );
        observe(|o| o.emit(json!({"event":"device_created"})));
    }
    result
}
unsafe extern "system" fn destroy_device(
    handle: vk::Device,
    alloc: *const vk::AllocationCallbacks<'_>,
) {
    if handle == vk::Device::null() {
        return;
    }
    let d = unsafe { device(handle.as_raw()) };
    let k = unsafe { key(handle.as_raw()) };
    observe(|o| o.tracker.destroy_device(handle.as_raw()));
    unsafe {
        (d.fns.destroy_device)(handle, alloc);
    }
    let mut state = lock(dispatch());
    if state
        .devices
        .get(&k)
        .is_some_and(|current| Arc::ptr_eq(current, &d))
    {
        state.devices.remove(&k);
    }
    drop(state);
    observe(|o| {
        o.emit(json!({"event":"device_destroyed"}));
    });
}

unsafe extern "system" fn create_shader(
    handle: vk::Device,
    info: *const vk::ShaderModuleCreateInfo<'_>,
    alloc: *const vk::AllocationCallbacks<'_>,
    out: *mut vk::ShaderModule,
) -> vk::Result {
    let d = unsafe { device(handle.as_raw()) };
    let result = unsafe { (d.fns.create_shader_module)(handle, info, alloc, out) };
    if result == vk::Result::SUCCESS {
        // Vulkan keeps pCode valid until this call returns. Never store it.
        let i = unsafe { &*info };
        let hash = if i.code_size > 0 && i.code_size <= MAX_SHADER_BYTES && !i.p_code.is_null() {
            let bytes = unsafe { std::slice::from_raw_parts(i.p_code.cast::<u8>(), i.code_size) };
            Some(format!("{:x}", Sha256::digest(bytes)))
        } else {
            None
        };
        let module = unsafe { *out };
        observe(|o| {
            let v = o
                .tracker
                .shader(handle.as_raw(), module.as_raw(), hash.as_deref());
            o.maybe(v);
        });
    }
    result
}
unsafe extern "system" fn destroy_shader(
    handle: vk::Device,
    module: vk::ShaderModule,
    alloc: *const vk::AllocationCallbacks<'_>,
) {
    let d = unsafe { device(handle.as_raw()) };
    // Retire before forwarding: another thread can receive this value as soon
    // as the downstream destroy frees it. Never erase that new generation.
    observe(|o| o.tracker.retire_shader(handle.as_raw(), module.as_raw()));
    unsafe {
        (d.fns.destroy_shader_module)(handle, module, alloc);
    }
}
unsafe extern "system" fn create_compute(
    handle: vk::Device,
    cache: vk::PipelineCache,
    count: u32,
    infos: *const vk::ComputePipelineCreateInfo<'_>,
    alloc: *const vk::AllocationCallbacks<'_>,
    out: *mut vk::Pipeline,
) -> vk::Result {
    let d = unsafe { device(handle.as_raw()) };
    let result =
        unsafe { (d.fns.create_compute_pipelines)(handle, cache, count, infos, alloc, out) };
    // Even an overall error can return valid pipelines in some output slots.
    if let (Some(inputs), Some(outputs)) = (unsafe { array(infos, count as usize) }, unsafe {
        array(out, count as usize)
    }) {
        for (i, p) in inputs.iter().zip(outputs) {
            if *p != vk::Pipeline::null() {
                observe(|o| {
                    let v = o.tracker.pipeline(
                        handle.as_raw(),
                        p.as_raw(),
                        i.stage.module.as_raw(),
                        !i.stage.p_next.is_null(),
                    );
                    o.maybe(v);
                });
            }
        }
    } else {
        unknown();
    }
    result
}
unsafe extern "system" fn destroy_pipeline(
    handle: vk::Device,
    pipeline: vk::Pipeline,
    alloc: *const vk::AllocationCallbacks<'_>,
) {
    let d = unsafe { device(handle.as_raw()) };
    observe(|o| {
        o.tracker
            .retire_pipeline(handle.as_raw(), pipeline.as_raw())
    });
    unsafe {
        (d.fns.destroy_pipeline)(handle, pipeline, alloc);
    }
}

unsafe extern "system" fn allocate_commands(
    handle: vk::Device,
    info: *const vk::CommandBufferAllocateInfo<'_>,
    out: *mut vk::CommandBuffer,
) -> vk::Result {
    let d = unsafe { device(handle.as_raw()) };
    let result = unsafe { (d.fns.allocate_command_buffers)(handle, info, out) };
    if result == vk::Result::SUCCESS {
        let i = unsafe { &*info };
        if let Some(commands) = unsafe { array(out, i.command_buffer_count as usize) } {
            observe(|o| {
                for c in commands {
                    o.tracker.allocate(
                        handle.as_raw(),
                        c.as_raw(),
                        i.command_pool.as_raw(),
                        i.level == vk::CommandBufferLevel::PRIMARY,
                    );
                }
            });
        } else {
            unknown();
        }
    }
    result
}
unsafe extern "system" fn free_commands(
    handle: vk::Device,
    pool: vk::CommandPool,
    count: u32,
    commands: *const vk::CommandBuffer,
) {
    let d = unsafe { device(handle.as_raw()) };
    // Copy handles before the downstream call invalidates their lifetimes.
    let handles = unsafe { array(commands, count as usize) }
        .map(|a| a.iter().map(|c| c.as_raw()).collect::<Vec<_>>());
    observe(|o| {
        if let Some(values) = handles {
            for h in values {
                o.tracker.commands.remove(&(handle.as_raw(), h));
            }
        } else {
            o.tracker.unknown();
            o.tracker.commands.retain(|&(d, _), _| d != handle.as_raw());
        }
    });
    unsafe {
        (d.fns.free_command_buffers)(handle, pool, count, commands);
    }
}
unsafe extern "system" fn begin_command(
    command: vk::CommandBuffer,
    info: *const vk::CommandBufferBeginInfo<'_>,
) -> vk::Result {
    let d = unsafe { device(command.as_raw()) };
    let result = unsafe { (d.fns.begin_command_buffer)(command, info) };
    if result == vk::Result::SUCCESS {
        observe(|o| o.tracker.reset(d.handle.as_raw(), command.as_raw()));
    } else {
        observe(|o| {
            o.tracker
                .mark_command_unknown(d.handle.as_raw(), command.as_raw())
        });
    }
    result
}
unsafe extern "system" fn end_command(command: vk::CommandBuffer) -> vk::Result {
    let d = unsafe { device(command.as_raw()) };
    let result = unsafe { (d.fns.end_command_buffer)(command) };
    observe(|o| {
        o.tracker.end(
            d.handle.as_raw(),
            command.as_raw(),
            result == vk::Result::SUCCESS,
        )
    });
    result
}
unsafe extern "system" fn reset_command(
    command: vk::CommandBuffer,
    flags: vk::CommandBufferResetFlags,
) -> vk::Result {
    let d = unsafe { device(command.as_raw()) };
    let result = unsafe { (d.fns.reset_command_buffer)(command, flags) };
    if result == vk::Result::SUCCESS {
        observe(|o| o.tracker.reset(d.handle.as_raw(), command.as_raw()));
    } else {
        observe(|o| {
            o.tracker
                .mark_command_unknown(d.handle.as_raw(), command.as_raw())
        });
    }
    result
}
unsafe extern "system" fn reset_pool(
    handle: vk::Device,
    pool: vk::CommandPool,
    flags: vk::CommandPoolResetFlags,
) -> vk::Result {
    let d = unsafe { device(handle.as_raw()) };
    let result = unsafe { (d.fns.reset_command_pool)(handle, pool, flags) };
    if result == vk::Result::SUCCESS {
        observe(|o| o.tracker.reset_pool(handle.as_raw(), pool.as_raw()));
    } else {
        observe(|o| o.tracker.unknown_pool(handle.as_raw(), pool.as_raw()));
    }
    result
}
unsafe extern "system" fn destroy_pool(
    handle: vk::Device,
    pool: vk::CommandPool,
    alloc: *const vk::AllocationCallbacks<'_>,
) {
    let d = unsafe { device(handle.as_raw()) };
    observe(|o| o.tracker.destroy_pool(handle.as_raw(), pool.as_raw()));
    unsafe {
        (d.fns.destroy_command_pool)(handle, pool, alloc);
    }
}
unsafe extern "system" fn bind_pipeline(
    command: vk::CommandBuffer,
    point: vk::PipelineBindPoint,
    pipeline: vk::Pipeline,
) {
    let d = unsafe { device(command.as_raw()) };
    unsafe {
        (d.fns.cmd_bind_pipeline)(command, point, pipeline);
    }
    if point == vk::PipelineBindPoint::COMPUTE {
        observe(|o| {
            o.tracker
                .bind(d.handle.as_raw(), command.as_raw(), pipeline.as_raw())
        });
    }
}
unsafe extern "system" fn cmd_dispatch(command: vk::CommandBuffer, x: u32, y: u32, z: u32) {
    let d = unsafe { device(command.as_raw()) };
    unsafe {
        (d.fns.cmd_dispatch)(command, x, y, z);
    }
    observe(|o| {
        let v = o
            .tracker
            .dispatch(d.handle.as_raw(), command.as_raw(), [x, y, z]);
        o.maybe(v);
    });
}
unsafe extern "system" fn dispatch_indirect(
    command: vk::CommandBuffer,
    buffer: vk::Buffer,
    offset: vk::DeviceSize,
) {
    let d = unsafe { device(command.as_raw()) };
    unsafe {
        (d.fns.cmd_dispatch_indirect)(command, buffer, offset);
    }
    observe(|o| {
        o.tracker
            .mark_command_unknown(d.handle.as_raw(), command.as_raw())
    });
}
unsafe extern "system" fn execute_commands(
    command: vk::CommandBuffer,
    count: u32,
    secondary: *const vk::CommandBuffer,
) {
    let d = unsafe { device(command.as_raw()) };
    unsafe {
        (d.fns.cmd_execute_commands)(command, count, secondary);
    }
    observe(|o| {
        o.tracker
            .shader_object_bind(d.handle.as_raw(), command.as_raw())
    });
}

// Preserve the exact extension alias used by the caller. Unsupported work is
// forwarded and marked unknown. Shader-object binds invalidate pipeline state.
unsafe fn extension(d: &DeviceDispatch, name: &CStr) -> unsafe extern "system" fn() {
    unsafe { (d.gpa)(d.handle, name.as_ptr()) }.unwrap_or_else(|| std::process::abort())
}
unsafe extern "system" fn bind_shaders(
    command: vk::CommandBuffer,
    count: u32,
    stages: *const vk::ShaderStageFlags,
    shaders: *const vk::ShaderEXT,
) {
    let d = unsafe { device(command.as_raw()) };
    let next: vk::PFN_vkCmdBindShadersEXT =
        unsafe { std::mem::transmute(extension(&d, c"vkCmdBindShadersEXT")) };
    unsafe {
        next(command, count, stages, shaders);
    }
    observe(|o| {
        o.tracker
            .shader_object_bind(d.handle.as_raw(), command.as_raw())
    });
}
macro_rules! dispatch_base_hook {
    ($name:ident,$api:expr) => {
        unsafe extern "system" fn $name(
            command: vk::CommandBuffer,
            bx: u32,
            by: u32,
            bz: u32,
            x: u32,
            y: u32,
            z: u32,
        ) {
            let d = unsafe { device(command.as_raw()) };
            let next: vk::PFN_vkCmdDispatchBase =
                unsafe { std::mem::transmute(extension(&d, $api)) };
            unsafe {
                next(command, bx, by, bz, x, y, z);
            }
            observe(|o| {
                o.tracker
                    .mark_command_unknown(d.handle.as_raw(), command.as_raw())
            });
        }
    };
}
dispatch_base_hook!(dispatch_base, c"vkCmdDispatchBase");
dispatch_base_hook!(dispatch_base_khr, c"vkCmdDispatchBaseKHR");
macro_rules! submit2_hook {
    ($name:ident,$api:expr) => {
        unsafe extern "system" fn $name(
            queue: vk::Queue,
            count: u32,
            infos: *const vk::SubmitInfo2<'_>,
            fence: vk::Fence,
        ) -> vk::Result {
            let d = unsafe { device(queue.as_raw()) };
            let next: vk::PFN_vkQueueSubmit2 = unsafe { std::mem::transmute(extension(&d, $api)) };
            let result = unsafe { next(queue, count, infos, fence) };
            unknown();
            result
        }
    };
}
submit2_hook!(submit2, c"vkQueueSubmit2");
submit2_hook!(submit2_khr, c"vkQueueSubmit2KHR");
unsafe extern "system" fn submit(
    queue: vk::Queue,
    count: u32,
    submits: *const vk::SubmitInfo<'_>,
    fence: vk::Fence,
) -> vk::Result {
    let d = unsafe { device(queue.as_raw()) };
    let mut handles = Vec::new();
    let mut complete = true;
    if let Some(batches) = unsafe { array(submits, count as usize) } {
        for b in batches {
            if let Some(cmds) =
                unsafe { array(b.p_command_buffers, b.command_buffer_count as usize) }
            {
                if handles.len() + cmds.len() > MAX_ARRAY {
                    complete = false;
                    break;
                }
                handles.extend(cmds.iter().map(|c| c.as_raw()));
            } else {
                complete = false;
            }
        }
    } else {
        complete = false;
    }
    let result = unsafe { (d.fns.queue_submit)(queue, count, submits, fence) };
    observe(|o| {
        if !complete {
            o.tracker.unknown();
        }
        let v = o.tracker.submission(
            d.handle.as_raw(),
            &handles,
            result == vk::Result::SUCCESS,
            complete,
        );
        o.maybe(v);
    });
    result
}

// Map hooks to untyped Vulkan function pointers only at the ABI boundary.
macro_rules! erased {
    ($f:ident) => {
        Some(unsafe {
            std::mem::transmute::<*const (), unsafe extern "system" fn()>($f as *const ())
        })
    };
}
#[cfg(test)]
mod forwarding_tests;
fn hook(name: &[u8]) -> vk::PFN_vkVoidFunction {
    match name {
        b"vkGetInstanceProcAddr" => erased!(instance_gpa),
        b"vkGetDeviceProcAddr" => erased!(device_gpa),
        b"vkCreateInstance" => erased!(create_instance),
        b"vkDestroyInstance" => erased!(destroy_instance),
        b"vkCreateDevice" => erased!(create_device),
        b"vkDestroyDevice" => erased!(destroy_device),
        b"vkCreateShaderModule" => erased!(create_shader),
        b"vkDestroyShaderModule" => erased!(destroy_shader),
        b"vkCreateComputePipelines" => erased!(create_compute),
        b"vkDestroyPipeline" => erased!(destroy_pipeline),
        b"vkAllocateCommandBuffers" => erased!(allocate_commands),
        b"vkFreeCommandBuffers" => erased!(free_commands),
        b"vkBeginCommandBuffer" => erased!(begin_command),
        b"vkEndCommandBuffer" => erased!(end_command),
        b"vkResetCommandBuffer" => erased!(reset_command),
        b"vkResetCommandPool" => erased!(reset_pool),
        b"vkDestroyCommandPool" => erased!(destroy_pool),
        b"vkCmdBindPipeline" => erased!(bind_pipeline),
        b"vkCmdDispatch" => erased!(cmd_dispatch),
        b"vkCmdDispatchIndirect" => erased!(dispatch_indirect),
        b"vkCmdExecuteCommands" => erased!(execute_commands),
        b"vkQueueSubmit" => erased!(submit),
        b"vkQueueSubmit2" => erased!(submit2),
        b"vkQueueSubmit2KHR" => erased!(submit2_khr),
        b"vkCmdDispatchBase" => erased!(dispatch_base),
        b"vkCmdDispatchBaseKHR" => erased!(dispatch_base_khr),
        b"vkCmdBindShadersEXT" => erased!(bind_shaders),
        _ => None,
    }
}
fn unsupported_query(name: &[u8]) {
    if matches!(
        name,
        b"vkCreateShadersEXT"
            | b"vkCmdBindShadersEXT"
            | b"vkQueueSubmit2"
            | b"vkQueueSubmit2KHR"
            | b"vkCmdDispatchBase"
            | b"vkCmdDispatchBaseKHR"
    ) {
        observe(|o| o.tracker.extension_queries = o.tracker.extension_queries.saturating_add(1));
    }
}

/// # Safety
/// Vulkan supplies a valid instance and a terminated function-name string.
// Keep these symbols private. Exported Vulkan names can be preempted by the
// loader on ELF systems. Interface 2 obtains the pointers through negotiation.
unsafe extern "system" fn instance_gpa(
    handle: vk::Instance,
    name: *const c_char,
) -> vk::PFN_vkVoidFunction {
    if name.is_null() {
        return None;
    }
    let bytes = unsafe { CStr::from_ptr(name) }.to_bytes();
    if handle == vk::Instance::null() {
        return match bytes {
            b"vkGetInstanceProcAddr" => erased!(instance_gpa),
            b"vkCreateInstance" => erased!(create_instance),
            _ => None,
        };
    }
    let d = unsafe { instance(handle.as_raw()) };
    let downstream = unsafe { (d.gpa)(handle, name) };
    downstream?;
    unsupported_query(bytes);
    hook(bytes).or(downstream)
}
/// # Safety
/// Vulkan supplies a valid device and a terminated function-name string.
unsafe extern "system" fn device_gpa(
    handle: vk::Device,
    name: *const c_char,
) -> vk::PFN_vkVoidFunction {
    if handle == vk::Device::null() || name.is_null() {
        return None;
    }
    let d = unsafe { device(handle.as_raw()) };
    let downstream = unsafe { (d.gpa)(handle, name) };
    downstream?;
    let bytes = unsafe { CStr::from_ptr(name) }.to_bytes();
    unsupported_query(bytes);
    hook(bytes).or(downstream)
}
unsafe extern "system" fn physical_gpa(
    handle: vk::Instance,
    name: *const c_char,
) -> vk::PFN_vkVoidFunction {
    if handle == vk::Instance::null() || name.is_null() {
        return None;
    }
    let d = unsafe { instance(handle.as_raw()) };
    if let Some(next) = d.physical {
        unsafe { next(handle, name) }
    } else {
        unsafe { (d.gpa)(handle, name) }
    }
}

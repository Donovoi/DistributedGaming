//! Exercise the ABI hooks with two independent downstream dispatch tables.
use super::*;

static CALLS: Mutex<Vec<Vec<u64>>> = Mutex::new(Vec::new());

unsafe extern "system" fn dispatch_a(c: vk::CommandBuffer, x: u32, y: u32, z: u32) {
    lock(&CALLS).push(vec![1, c.as_raw(), x.into(), y.into(), z.into()]);
}
unsafe extern "system" fn dispatch_b(c: vk::CommandBuffer, x: u32, y: u32, z: u32) {
    lock(&CALLS).push(vec![2, c.as_raw(), x.into(), y.into(), z.into()]);
}
unsafe extern "system" fn submit_failure(
    q: vk::Queue,
    n: u32,
    p: *const vk::SubmitInfo<'_>,
    f: vk::Fence,
) -> vk::Result {
    lock(&CALLS).push(vec![3, q.as_raw(), n.into(), p as u64, f.as_raw()]);
    vk::Result::ERROR_DEVICE_LOST
}
unsafe extern "system" fn shader_failure(
    d: vk::Device,
    p: *const vk::ShaderModuleCreateInfo<'_>,
    a: *const vk::AllocationCallbacks<'_>,
    o: *mut vk::ShaderModule,
) -> vk::Result {
    lock(&CALLS).push(vec![4, d.as_raw(), p as u64, a as u64, o as u64]);
    vk::Result::ERROR_OUT_OF_DEVICE_MEMORY
}
unsafe extern "system" fn partial_pipelines(
    d: vk::Device,
    c: vk::PipelineCache,
    n: u32,
    p: *const vk::ComputePipelineCreateInfo<'_>,
    a: *const vk::AllocationCallbacks<'_>,
    o: *mut vk::Pipeline,
) -> vk::Result {
    lock(&CALLS).push(vec![
        5,
        d.as_raw(),
        c.as_raw(),
        n.into(),
        p as u64,
        a as u64,
        o as u64,
    ]);
    if n == 2 {
        unsafe {
            *o = vk::Pipeline::from_raw(73);
            *o.add(1) = vk::Pipeline::null();
        }
    }
    vk::Result::ERROR_OUT_OF_DEVICE_MEMORY
}
unsafe extern "system" fn begin_failure(
    c: vk::CommandBuffer,
    p: *const vk::CommandBufferBeginInfo<'_>,
) -> vk::Result {
    lock(&CALLS).push(vec![6, c.as_raw(), p as u64]);
    vk::Result::ERROR_OUT_OF_HOST_MEMORY
}
unsafe extern "system" fn creation_failure(
    p: *const vk::InstanceCreateInfo<'_>,
    a: *const vk::AllocationCallbacks<'_>,
    o: *mut vk::Instance,
) -> vk::Result {
    lock(&CALLS).push(vec![7, p as u64, a as u64, o as u64]);
    vk::Result::ERROR_INCOMPATIBLE_DRIVER
}
unsafe extern "system" fn mock_igpa(_: vk::Instance, n: *const c_char) -> vk::PFN_vkVoidFunction {
    if unsafe { CStr::from_ptr(n) }.to_bytes() == b"vkCreateInstance" {
        erased!(creation_failure)
    } else {
        None
    }
}
unsafe extern "system" fn mock_gpa(_: vk::Device, n: *const c_char) -> vk::PFN_vkVoidFunction {
    match unsafe { CStr::from_ptr(n) }.to_bytes() {
        b"vkCmdDispatch" => erased!(dispatch_a),
        b"vkQueueSubmit" => erased!(submit_failure),
        b"vkCreateShaderModule" => erased!(shader_failure),
        b"vkCreateComputePipelines" => erased!(partial_pipelines),
        b"vkBeginCommandBuffer" => erased!(begin_failure),
        _ => None,
    }
}
fn mock_device(handle: vk::Device, second: bool) -> Arc<DeviceDispatch> {
    let mut fns = ash::DeviceFnV1_0::load(|n| unsafe {
        mock_gpa(handle, n.as_ptr()).map_or(std::ptr::null(), |f| f as *const c_void)
    });
    if second {
        fns.cmd_dispatch = dispatch_b;
    }
    Arc::new(DeviceDispatch {
        handle,
        gpa: mock_gpa,
        fns,
    })
}

#[test]
fn hooks_keep_arguments_results_and_device_routing_after_observer_failure() {
    let a = Box::new(0xabc001usize);
    let b = Box::new(0xabc002usize);
    let da = vk::Device::from_raw((&*a as *const usize) as u64);
    let db = vk::Device::from_raw((&*b as *const usize) as u64);
    let ca = vk::CommandBuffer::from_raw(da.as_raw());
    let cb = vk::CommandBuffer::from_raw(db.as_raw());
    lock(dispatch()).devices.insert(*a, mock_device(da, false));
    lock(dispatch()).devices.insert(*b, mock_device(db, true));
    observe(|o| {
        o.journal = None;
        o.missing_log = true;
        o.tracker.allocate(da.as_raw(), ca.as_raw(), 1, true);
    });
    unsafe {
        cmd_dispatch(ca, 7, 8, 9);
    }
    observe(|_| panic!("injected observer panic"));
    unsafe {
        cmd_dispatch(cb, 11, 12, 13);
    }
    let q = vk::Queue::from_raw(db.as_raw());
    let fence = vk::Fence::from_raw(71);
    assert_eq!(
        unsafe { submit(q, 0, std::ptr::null(), fence) },
        vk::Result::ERROR_DEVICE_LOST
    );
    let shader_info = vk::ShaderModuleCreateInfo::default();
    let mut shader = vk::ShaderModule::from_raw(72);
    assert_eq!(
        unsafe { create_shader(da, &shader_info, std::ptr::null(), &mut shader) },
        vk::Result::ERROR_OUT_OF_DEVICE_MEMORY
    );
    assert_eq!(shader.as_raw(), 72);
    let pipeline_infos = [vk::ComputePipelineCreateInfo::default(); 2];
    let mut pipelines = [vk::Pipeline::null(); 2];
    let cache = vk::PipelineCache::from_raw(74);
    assert_eq!(
        unsafe {
            create_compute(
                da,
                cache,
                2,
                pipeline_infos.as_ptr(),
                std::ptr::null(),
                pipelines.as_mut_ptr(),
            )
        },
        vk::Result::ERROR_OUT_OF_DEVICE_MEMORY
    );
    assert_eq!(pipelines[0].as_raw(), 73);
    assert_eq!(pipelines[1], vk::Pipeline::null());
    let begin = vk::CommandBufferBeginInfo::default();
    assert_eq!(
        unsafe { begin_command(ca, &begin) },
        vk::Result::ERROR_OUT_OF_HOST_MEMORY
    );
    let mut state = None;
    observe(|o| {
        state = Some((
            o.tracker.commands[&(da.as_raw(), ca.as_raw())].unknown,
            o.tracker.pipelines.contains_key(&(da.as_raw(), 73)),
        ));
    });
    assert_eq!(state, Some((true, true)));
    assert!(PANICKED.load(Ordering::Relaxed));
    assert!(unsafe { device_gpa(da, c"vkUnsupportedForTest".as_ptr()) }.is_none());
    assert!(unsafe { array::<u32>(std::ptr::null(), 0) }
        .unwrap()
        .is_empty());
    assert!(unsafe { array::<u32>(std::ptr::null(), 1) }.is_none());
    assert!(unsafe { array::<u32>(std::ptr::null(), MAX_ARRAY + 1) }.is_none());
    let mut link = abi::InstanceLink {
        next: std::ptr::null_mut(),
        instance_gpa: mock_igpa,
        physical_gpa: None,
    };
    let mut chain = abi::InstanceCreate {
        s_type: vk::StructureType::LOADER_INSTANCE_CREATE_INFO,
        next: std::ptr::null(),
        function: 0,
        data: abi::InstanceUnion { link: &mut link },
    };
    let info = vk::InstanceCreateInfo {
        p_next: (&mut chain as *mut abi::InstanceCreate).cast(),
        ..Default::default()
    };
    let mut output = vk::Instance::null();
    assert_eq!(
        unsafe { create_instance(&info, std::ptr::null(), &mut output) },
        vk::Result::ERROR_INCOMPATIBLE_DRIVER
    );
    assert!(unsafe { chain.data.link }.is_null());
    let calls = lock(&CALLS);
    assert_eq!(
        calls.as_slice(),
        &[
            vec![1, ca.as_raw(), 7, 8, 9],
            vec![2, cb.as_raw(), 11, 12, 13],
            vec![3, q.as_raw(), 0, 0, 71],
            vec![
                4,
                da.as_raw(),
                &shader_info as *const _ as u64,
                0,
                &mut shader as *mut _ as u64
            ],
            vec![
                5,
                da.as_raw(),
                74,
                2,
                pipeline_infos.as_ptr() as u64,
                0,
                pipelines.as_ptr() as u64
            ],
            vec![6, ca.as_raw(), &begin as *const _ as u64],
            vec![7, &info as *const _ as u64, 0, &mut output as *mut _ as u64],
        ]
    );
    drop(calls);
    lock(dispatch()).devices.remove(&*a);
    lock(dispatch()).devices.remove(&*b);
}

unsafe extern "system" fn reuse_shader(
    d: vk::Device,
    s: vk::ShaderModule,
    _: *const vk::AllocationCallbacks<'_>,
) {
    observe(|o| {
        o.tracker.shader(d.as_raw(), s.as_raw(), Some("new module"));
    });
}
unsafe extern "system" fn reuse_pipeline(
    d: vk::Device,
    p: vk::Pipeline,
    _: *const vk::AllocationCallbacks<'_>,
) {
    observe(|o| {
        o.tracker.pipeline(d.as_raw(), p.as_raw(), 2, false);
    });
}
unsafe extern "system" fn reuse_commands(
    d: vk::Device,
    p: vk::CommandPool,
    n: u32,
    c: *const vk::CommandBuffer,
) {
    let handles = unsafe { array(c, n as usize) }.unwrap();
    observe(|o| {
        for h in handles {
            o.tracker
                .allocate(d.as_raw(), h.as_raw(), p.as_raw() + 1, true);
        }
    });
}
unsafe extern "system" fn reuse_pool(
    d: vk::Device,
    p: vk::CommandPool,
    _: *const vk::AllocationCallbacks<'_>,
) {
    observe(|o| o.tracker.allocate(d.as_raw(), 4, p.as_raw() + 1, true));
}
#[test]
fn destruction_does_not_erase_a_reused_handle_created_downstream() {
    let keybox = Box::new(0xabc003usize);
    let d = vk::Device::from_raw((&*keybox as *const usize) as u64);
    let mut fns = ash::DeviceFnV1_0::load(|_| std::ptr::null());
    fns.destroy_shader_module = reuse_shader;
    fns.destroy_pipeline = reuse_pipeline;
    fns.free_command_buffers = reuse_commands;
    fns.destroy_command_pool = reuse_pool;
    lock(dispatch()).devices.insert(
        *keybox,
        Arc::new(DeviceDispatch {
            handle: d,
            gpa: mock_gpa,
            fns,
        }),
    );
    let mut old_ids = None;
    observe(|o| {
        o.tracker.shader(d.as_raw(), 2, Some(track::TARGET));
        o.tracker.pipeline(d.as_raw(), 3, 2, false);
        o.tracker.allocate(d.as_raw(), 4, 5, true);
        old_ids = Some((
            o.tracker.pipelines[&(d.as_raw(), 3)].id,
            o.tracker.commands[&(d.as_raw(), 4)].id,
        ));
    });
    unsafe {
        destroy_shader(d, vk::ShaderModule::from_raw(2), std::ptr::null());
    }
    unsafe {
        destroy_pipeline(d, vk::Pipeline::from_raw(3), std::ptr::null());
    }
    let c = vk::CommandBuffer::from_raw(4);
    let pool = vk::CommandPool::from_raw(5);
    unsafe {
        free_commands(d, pool, 1, &c);
    }
    let mut after_free = None;
    observe(|o| after_free = Some(o.tracker.commands[&(d.as_raw(), 4)].id));
    unsafe {
        destroy_pool(d, vk::CommandPool::from_raw(6), std::ptr::null());
    }
    let mut state = None;
    observe(|o| {
        state = Some((
            o.tracker.shaders[&(d.as_raw(), 2)].identity,
            o.tracker.pipelines[&(d.as_raw(), 3)].id,
            o.tracker.commands[&(d.as_raw(), 4)].id,
        ))
    });
    let (shader, pipeline_id, command_id) = state.unwrap();
    assert_eq!(shader, track::Identity::Other);
    assert_ne!(pipeline_id, old_ids.unwrap().0);
    assert_ne!(after_free.unwrap(), old_ids.unwrap().1);
    assert_ne!(command_id, after_free.unwrap());
    lock(dispatch()).devices.remove(&*keybox);
}

static EXT_CALLS: Mutex<Vec<Vec<u64>>> = Mutex::new(Vec::new());
unsafe extern "system" fn ext_bind(
    c: vk::CommandBuffer,
    n: u32,
    s: *const vk::ShaderStageFlags,
    h: *const vk::ShaderEXT,
) {
    lock(&EXT_CALLS).push(vec![1, c.as_raw(), n.into(), s as u64, h as u64]);
}
macro_rules! mock_base {
    ($name:ident,$id:expr) => {
        unsafe extern "system" fn $name(
            c: vk::CommandBuffer,
            bx: u32,
            by: u32,
            bz: u32,
            x: u32,
            y: u32,
            z: u32,
        ) {
            lock(&EXT_CALLS).push(vec![
                $id,
                c.as_raw(),
                bx.into(),
                by.into(),
                bz.into(),
                x.into(),
                y.into(),
                z.into(),
            ]);
        }
    };
}
mock_base!(ext_base, 2);
mock_base!(ext_base_khr, 3);
macro_rules! mock_submit2 {
    ($name:ident,$id:expr) => {
        unsafe extern "system" fn $name(
            q: vk::Queue,
            n: u32,
            p: *const vk::SubmitInfo2<'_>,
            f: vk::Fence,
        ) -> vk::Result {
            lock(&EXT_CALLS).push(vec![$id, q.as_raw(), n.into(), p as u64, f.as_raw()]);
            vk::Result::ERROR_DEVICE_LOST
        }
    };
}
mock_submit2!(ext_submit2, 4);
mock_submit2!(ext_submit2_khr, 5);
unsafe extern "system" fn ext_dispatch(c: vk::CommandBuffer, x: u32, y: u32, z: u32) {
    lock(&EXT_CALLS).push(vec![6, c.as_raw(), x.into(), y.into(), z.into()]);
}
unsafe extern "system" fn ext_gpa(_: vk::Device, n: *const c_char) -> vk::PFN_vkVoidFunction {
    match unsafe { CStr::from_ptr(n) }.to_bytes() {
        b"vkCmdBindShadersEXT" => erased!(ext_bind),
        b"vkCmdDispatchBase" => erased!(ext_base),
        b"vkCmdDispatchBaseKHR" => erased!(ext_base_khr),
        b"vkQueueSubmit2" => erased!(ext_submit2),
        b"vkQueueSubmit2KHR" => erased!(ext_submit2_khr),
        _ => None,
    }
}
#[test]
fn extension_aliases_forward_and_shader_objects_clear_pipeline_identity() {
    let keybox = Box::new(0xabc004usize);
    let d = vk::Device::from_raw((&*keybox as *const usize) as u64);
    let c = vk::CommandBuffer::from_raw(d.as_raw());
    let q = vk::Queue::from_raw(d.as_raw());
    let mut fns = ash::DeviceFnV1_0::load(|_| std::ptr::null());
    fns.cmd_dispatch = ext_dispatch;
    lock(dispatch()).devices.insert(
        *keybox,
        Arc::new(DeviceDispatch {
            handle: d,
            gpa: ext_gpa,
            fns,
        }),
    );
    observe(|o| {
        o.tracker.shader(d.as_raw(), 2, Some(track::TARGET));
        o.tracker.pipeline(d.as_raw(), 3, 2, false);
        o.tracker.allocate(d.as_raw(), c.as_raw(), 5, true);
        o.tracker.bind(d.as_raw(), c.as_raw(), 3);
    });
    unsafe {
        cmd_dispatch(c, 1, 1, 1);
        bind_shaders(c, 0, std::ptr::null(), std::ptr::null());
        cmd_dispatch(c, 1, 1, 1);
    }
    unsafe {
        dispatch_base(c, 1, 2, 3, 4, 5, 6);
        dispatch_base_khr(c, 7, 8, 9, 10, 11, 12);
    }
    let fence = vk::Fence::from_raw(91);
    assert_eq!(
        unsafe { submit2(q, 0, std::ptr::null(), fence) },
        vk::Result::ERROR_DEVICE_LOST
    );
    assert_eq!(
        unsafe { submit2_khr(q, 0, std::ptr::null(), fence) },
        vk::Result::ERROR_DEVICE_LOST
    );
    let mut state = None;
    observe(|o| {
        let command = &o.tracker.commands[&(d.as_raw(), c.as_raw())];
        state = Some((
            command.pipeline.is_none(),
            command.unknown,
            command.target_dispatches,
        ));
    });
    assert_eq!(state, Some((true, true, 1)));
    assert_eq!(
        lock(&EXT_CALLS).as_slice(),
        &[
            vec![6, c.as_raw(), 1, 1, 1],
            vec![1, c.as_raw(), 0, 0, 0],
            vec![6, c.as_raw(), 1, 1, 1],
            vec![2, c.as_raw(), 1, 2, 3, 4, 5, 6],
            vec![3, c.as_raw(), 7, 8, 9, 10, 11, 12],
            vec![4, q.as_raw(), 0, 0, 91],
            vec![5, q.as_raw(), 0, 0, 91],
        ]
    );
    lock(dispatch()).devices.remove(&*keybox);
}

unsafe extern "system" fn reuse_device(d: vk::Device, _: *const vk::AllocationCallbacks<'_>) {
    let k = unsafe { key(d.as_raw()) };
    lock(dispatch()).devices.insert(k, mock_device(d, true));
    observe(|o| {
        o.tracker.shader(d.as_raw(), 2, Some(track::TARGET));
    });
}
unsafe extern "system" fn reuse_instance(i: vk::Instance, _: *const vk::AllocationCallbacks<'_>) {
    let k = unsafe { key(i.as_raw()) };
    let fns = ash::InstanceFnV1_0::load(|_| std::ptr::null());
    lock(dispatch()).instances.insert(
        k,
        Arc::new(InstanceDispatch {
            handle: i,
            gpa: mock_igpa,
            physical: None,
            fns,
        }),
    );
}
#[test]
fn destroy_keeps_new_dispatch_generations_and_device_children() {
    let db = Box::new(0xabc005usize);
    let ib = Box::new(0xabc006usize);
    let d = vk::Device::from_raw((&*db as *const usize) as u64);
    let i = vk::Instance::from_raw((&*ib as *const usize) as u64);
    let mut dfns = ash::DeviceFnV1_0::load(|_| std::ptr::null());
    dfns.destroy_device = reuse_device;
    let old_device = Arc::new(DeviceDispatch {
        handle: d,
        gpa: mock_gpa,
        fns: dfns,
    });
    let mut ifns = ash::InstanceFnV1_0::load(|_| std::ptr::null());
    ifns.destroy_instance = reuse_instance;
    let old_instance = Arc::new(InstanceDispatch {
        handle: i,
        gpa: mock_igpa,
        physical: None,
        fns: ifns,
    });
    lock(dispatch()).devices.insert(*db, old_device.clone());
    lock(dispatch()).instances.insert(*ib, old_instance.clone());
    observe(|o| {
        o.tracker.shader(d.as_raw(), 2, Some("old module"));
    });
    unsafe {
        destroy_device(d, std::ptr::null());
        destroy_instance(i, std::ptr::null());
    }
    assert!(!Arc::ptr_eq(&lock(dispatch()).devices[&*db], &old_device));
    assert!(!Arc::ptr_eq(
        &lock(dispatch()).instances[&*ib],
        &old_instance
    ));
    let mut identity = None;
    observe(|o| identity = o.tracker.shaders.get(&(d.as_raw(), 2)).map(|s| s.identity));
    assert_eq!(identity, Some(track::Identity::Target));
    lock(dispatch()).devices.remove(&*db);
    lock(dispatch()).instances.remove(&*ib);
}

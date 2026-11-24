//! Loader-only ABI from Vulkan-Headers vk_layer.h. Vulkan API types come from ash.
use ash::vk;
use std::ffi::c_void;

pub type PhysicalGpa = vk::PFN_vkGetInstanceProcAddr;

#[repr(C)]
pub struct Negotiate {
    pub s_type: i32,
    pub next: *mut c_void,
    pub version: u32,
    pub instance_gpa: Option<vk::PFN_vkGetInstanceProcAddr>,
    pub device_gpa: Option<vk::PFN_vkGetDeviceProcAddr>,
    pub physical_gpa: Option<PhysicalGpa>,
}

#[repr(C)]
pub struct InstanceLink {
    pub next: *mut InstanceLink,
    pub instance_gpa: vk::PFN_vkGetInstanceProcAddr,
    pub physical_gpa: Option<PhysicalGpa>,
}

#[repr(C)]
pub struct DeviceLink {
    pub next: *mut DeviceLink,
    pub instance_gpa: vk::PFN_vkGetInstanceProcAddr,
    pub device_gpa: vk::PFN_vkGetDeviceProcAddr,
}

#[repr(C)]
pub union InstanceUnion {
    pub link: *mut InstanceLink,
    // vk_layer.h also stores two function pointers in this union.
    pub storage: [usize; 2],
}

#[repr(C)]
pub union DeviceUnion {
    pub link: *mut DeviceLink,
    pub storage: usize,
}

#[repr(C)]
pub struct InstanceCreate {
    pub s_type: vk::StructureType,
    pub next: *const c_void,
    pub function: i32,
    pub data: InstanceUnion,
}

#[repr(C)]
pub struct DeviceCreate {
    pub s_type: vk::StructureType,
    pub next: *const c_void,
    pub function: i32,
    pub data: DeviceUnion,
}

// A valid loader owns this chain for the duration of CreateInstance/CreateDevice.
// The layer may advance only its link pointer, as required by the loader ABI.
pub unsafe fn instance_link(mut node: *const c_void) -> Option<*mut InstanceCreate> {
    for _ in 0..256 {
        if node.is_null() {
            return None;
        }
        let base = unsafe { &*node.cast::<vk::BaseInStructure<'_>>() };
        if base.s_type == vk::StructureType::LOADER_INSTANCE_CREATE_INFO {
            let info = node.cast::<InstanceCreate>().cast_mut();
            if unsafe { (*info).function == 0 } {
                return Some(info);
            }
        }
        node = base.p_next.cast();
    }
    None
}

pub unsafe fn device_link(mut node: *const c_void) -> Option<*mut DeviceCreate> {
    for _ in 0..256 {
        if node.is_null() {
            return None;
        }
        let base = unsafe { &*node.cast::<vk::BaseInStructure<'_>>() };
        if base.s_type == vk::StructureType::LOADER_DEVICE_CREATE_INFO {
            let info = node.cast::<DeviceCreate>().cast_mut();
            if unsafe { (*info).function == 0 } {
                return Some(info);
            }
        }
        node = base.p_next.cast();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn linux_x64_loader_layout() {
        assert_eq!(std::mem::size_of::<usize>(), 8);
        assert_eq!(std::mem::size_of::<Negotiate>(), 48);
        assert_eq!(std::mem::offset_of!(Negotiate, instance_gpa), 24);
        assert_eq!(std::mem::size_of::<InstanceCreate>(), 40);
        assert_eq!(std::mem::offset_of!(InstanceCreate, data), 24);
        assert_eq!(std::mem::size_of::<DeviceCreate>(), 32);
        assert_eq!(std::mem::offset_of!(DeviceCreate, data), 24);
    }
}

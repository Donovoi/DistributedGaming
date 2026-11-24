use sha2::{Digest, Sha256};
use std::{
    borrow::Cow,
    error::Error,
    io::Read,
    path::Path,
    sync::mpsc,
    time::{Duration, Instant},
};
use wgpu::util::DeviceExt;

fn bytes(path: &Path, size: usize) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut data = Vec::new();
    std::fs::File::open(path)?
        .take(size as u64 + 1)
        .read_to_end(&mut data)?;
    if data.len() != size {
        return Err("Wrong input size".into());
    }
    Ok(data)
}

fn hash(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}

fn identity_matches(histogram: &[u8], uniform: &[u8], native: &[u8]) -> bool {
    hash(histogram) == "a2082ed95408d0c467eba82096d76ba615fab623a289191c0966972250ff24cc"
        && hash(uniform) == "6f2bbde6bc0e70da2c40539179055dd8a4a38eb58e3cb987356c5ee1ca1a9a5e"
        && hash(native) == "60f5c1a960699456db2ebfda0da480133311202723e4e1eb5f9d9114ef9e3f3c"
}

struct Case {
    histogram: Vec<u8>,
    uniform: Vec<u8>,
    native: Vec<u8>,
}

fn load_case(dir: &Path) -> Result<Case, Box<dyn Error>> {
    let histogram = bytes(&dir.join("session-0-histogram.bin"), 1024)?;
    let uniform = bytes(&dir.join("session-0-uniform.bin"), 32)?;
    let native = bytes(&dir.join("session-0-output.bin"), 4)?;
    if !identity_matches(&histogram, &uniform, &native) {
        return Err("Reference identity mismatch".into());
    }
    Ok(Case {
        histogram,
        uniform,
        native,
    })
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() == 1 && args[0] == "--help" {
        println!("dg-reduction-probe PRIVATE_REFERENCE_DIRECTORY\nFixed CS2 capture case only. Run on a remote NVIDIA Vulkan GPU.\nThis is a reimplementation check, not native shader execution or a game speedup.");
        return Ok(());
    }
    if args.len() != 1 {
        return Err("Use --help for the fixed-case procedure".into());
    }
    let Case {
        histogram,
        uniform,
        native,
    } = load_case(Path::new(&args[0]))?;
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        ..Default::default()
    });
    let adapters = instance.enumerate_adapters(wgpu::Backends::VULKAN);
    let adapter = adapters
        .into_iter()
        .find(|a| {
            let i = a.get_info();
            i.vendor == 0x10de && i.device_type == wgpu::DeviceType::DiscreteGpu
        })
        .ok_or("No NVIDIA discrete Vulkan adapter")?;
    let info = adapter.get_info();
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("Private reduction probe"),
        ..Default::default()
    }))?;
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Reimplemented reduction"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("reduce.wgsl"))),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &shader,
        entry_point: Some("reduce"),
        compilation_options: Default::default(),
        cache: None,
    });
    let input = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: &histogram,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    });
    let params = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: &uniform,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_SRC,
    });
    let mut reports = Vec::new();
    for sentinel in [0u32, u32::MAX, 0x12345678] {
        let initial: Vec<u8> = [0xdeadbeefu32, sentinel, 0xfeedface]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect();
        let output = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: &initial,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        });
        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 1068,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: input.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: params.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: output.as_entire_binding(),
                },
            ],
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&output, 0, &staging, 0, 12);
        encoder.copy_buffer_to_buffer(&input, 0, &staging, 12, 1024);
        encoder.copy_buffer_to_buffer(&params, 0, &staging, 1036, 32);
        queue.submit([encoder.finish()]);
        let (tx, rx) = mpsc::channel();
        staging.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        let start = Instant::now();
        loop {
            device.poll(wgpu::PollType::Poll)?;
            match rx.try_recv() {
                Ok(result) => {
                    result?;
                    break;
                }
                Err(mpsc::TryRecvError::Empty) => (),
                Err(e) => return Err(e.into()),
            }
            if start.elapsed() > Duration::from_secs(5) {
                return Err("GPU readback timeout".into());
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        let view = staging.slice(..).get_mapped_range();
        let result = view[4..8].to_vec();
        let guards = view[..4] == initial[..4] && view[8..12] == initial[8..12];
        let unchanged = view[12..1036] == histogram && view[1036..] == uniform;
        reports.push(serde_json::json!({"sentinel":sentinel,"exact":result==native,
            "output_bits":u32::from_le_bytes(result.try_into().unwrap()),"guards_unchanged":guards,"inputs_unchanged":unchanged}));
        drop(view);
        staging.unmap();
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "adapter":info.name,"backend":format!("{:?}",info.backend),"driver":info.driver_info,
            "histogram_sha256":hash(&histogram),"uniform_sha256":hash(&uniform),
            "native_output_bits":u32::from_le_bytes(native.try_into().unwrap()),
            "wgsl_sha256":hash(include_bytes!("reduce.wgsl")),"reimplementation":true,
            "profile":"cs2-build-26879581-capture-1-event-6905",
            "native_shader_sha256":"5b112e9919dff8911604f606a636b324e197cde9797196efd604b5f9ca48f9c7",
            "source_capture_sha256":"1ed0d19496074b82c2e04819f43529d08a1926e28a115deb7325038d1781cb23",
            "original_shader_executed":false,"game_speedup_proven":false,"runs":reports,
        }))?
    );
    if reports.iter().any(|r| {
        r["exact"] != true || r["guards_unchanged"] != true || r["inputs_unchanged"] != true
    }) {
        return Err("Exact verification failed".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn rejects_short_and_long_inputs() -> Result<(), Box<dyn Error>> {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let path = std::env::temp_dir().join(format!("dg-probe-{}-{nonce}", std::process::id()));
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)?;
        file.write_all(&[0; 1023])?;
        drop(file);
        let short = bytes(&path, 1024).is_err();
        std::fs::write(&path, [0; 1025])?;
        let long = bytes(&path, 1024).is_err();
        std::fs::write(&path, [0; 1024])?;
        let exact_size = bytes(&path, 1024)?.len() == 1024;
        std::fs::remove_file(&path)?;
        assert!(short && long && exact_size);
        Ok(())
    }

    #[test]
    fn correct_sizes_do_not_establish_identity() {
        assert!(!identity_matches(&[0; 1024], &[0; 32], &[0; 4]));
    }

    #[test]
    fn missing_case_fails_before_gpu_setup() {
        assert!(load_case(Path::new("missing-dg-probe-case/absent")).is_err());
    }
}

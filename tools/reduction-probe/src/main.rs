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

fn known_identity(histogram: &str, uniform: &str, native: &str) -> Option<serde_json::Value> {
    let cases: serde_json::Value = serde_json::from_str(include_str!("../cases.json")).ok()?;
    cases
        .as_array()?
        .iter()
        .find(|case| {
            case["histogram_sha256"] == histogram
                && case["uniform_sha256"] == uniform
                && case["native_output_sha256"] == native
        })
        .cloned()
}

struct Case {
    histogram: Vec<u8>,
    uniform: Vec<u8>,
    native: Vec<u8>,
    identity: serde_json::Value,
}

fn load_case(dir: &Path) -> Result<Case, Box<dyn Error>> {
    let histogram = bytes(&dir.join("session-0-histogram.bin"), 1024)?;
    let uniform = bytes(&dir.join("session-0-uniform.bin"), 32)?;
    let native = bytes(&dir.join("session-0-output.bin"), 4)?;
    let identity = known_identity(&hash(&histogram), &hash(&uniform), &hash(&native))
        .ok_or("Reference identity mismatch")?;
    Ok(Case {
        histogram,
        uniform,
        native,
        identity,
    })
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() == 1 && args[0] == "--help" {
        println!("dg-reduction-probe PRIVATE_REFERENCE_DIRECTORY\nCompiled fixed cases only. Run on a remote NVIDIA Vulkan GPU.\nThis is a reimplementation check, not native shader execution or a game speedup.");
        return Ok(());
    }
    if args.len() != 1 {
        return Err("Use --help for the fixed-case procedure".into());
    }
    let Case {
        histogram,
        uniform,
        native,
        identity,
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
            "profile":identity["id"],"origin":identity["origin"],
            "cases_sha256":hash(include_bytes!("../cases.json")),
            "native_shader_sha256":"5b112e9919dff8911604f606a636b324e197cde9797196efd604b5f9ca48f9c7",
            "source_capture_sha256":identity["source_capture_sha256"],
            "reference_method":"Original pinned shader with fixed isolated buffers",
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
        assert!(known_identity(&hash(&[0; 1024]), &hash(&[0; 32]), &hash(&[0; 4])).is_none());
    }

    #[test]
    fn missing_case_fails_before_gpu_setup() {
        assert!(load_case(Path::new("missing-dg-probe-case/absent")).is_err());
    }

    #[test]
    fn fixed_fixture_mutations_fail_before_gpu_setup() -> Result<(), Box<dyn Error>> {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("dg-case-{}-{nonce}", std::process::id()));
        std::fs::create_dir(&dir)?;
        let histogram = vec![0u8; 1024];
        let mut uniform: Vec<u8> = [1280u32, 720, (-8.0f32).to_bits(), 13.0f32.to_bits()]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect();
        uniform.resize(32, 0);
        let native = 998244352u32.to_le_bytes().to_vec();
        let files = [
            ("histogram", histogram),
            ("uniform", uniform),
            ("output", native),
        ];
        for (name, data) in &files {
            std::fs::write(dir.join(format!("session-0-{name}.bin")), data)?;
        }
        let valid = load_case(&dir)?.identity["id"] == "zero";
        let mut rejected = Vec::new();
        for (name, data) in &files {
            let path = dir.join(format!("session-0-{name}.bin"));
            let mut altered = data.clone();
            altered[0] ^= 1;
            std::fs::write(&path, altered)?;
            rejected.push(load_case(&dir).is_err());
            for size in [data.len() - 1, data.len() + 1] {
                std::fs::write(&path, vec![0u8; size])?;
                rejected.push(load_case(&dir).is_err());
            }
            std::fs::write(path, data)?;
        }
        std::fs::remove_dir_all(dir)?;
        assert!(valid && rejected.len() == 9 && rejected.into_iter().all(|r| r));
        Ok(())
    }

    #[test]
    fn case_identity_requires_one_complete_tuple() {
        let cases: serde_json::Value = serde_json::from_str(include_str!("../cases.json")).unwrap();
        let first = &cases[0];
        let second = &cases[1];
        let h = first["histogram_sha256"].as_str().unwrap();
        let u = first["uniform_sha256"].as_str().unwrap();
        let n = first["native_output_sha256"].as_str().unwrap();
        assert!(known_identity(h, u, n).is_some());
        assert!(known_identity(second["histogram_sha256"].as_str().unwrap(), u, n).is_none());
        assert!(known_identity(h, u, &hash(&[0; 4])).is_none());
    }

    #[test]
    fn compiled_reference_bits_agree_with_hashes() {
        let cases: serde_json::Value = serde_json::from_str(include_str!("../cases.json")).unwrap();
        let mut ids = std::collections::HashSet::new();
        let mut tuples = std::collections::HashSet::new();
        for case in cases.as_array().unwrap() {
            assert!(ids.insert(case["id"].as_str().unwrap()));
            let h = case["histogram_sha256"].as_str().unwrap();
            let u = case["uniform_sha256"].as_str().unwrap();
            let n = case["native_output_sha256"].as_str().unwrap();
            assert!(tuples.insert((h, u, n)));
            for digest in [h, u, n] {
                assert_eq!(digest.len(), 64);
                assert!(digest.bytes().all(|b| b.is_ascii_hexdigit()));
            }
            let bits = u32::try_from(case["native_output_bits"].as_u64().unwrap()).unwrap();
            assert_eq!(hash(&bits.to_le_bytes()), n);
        }
    }
}

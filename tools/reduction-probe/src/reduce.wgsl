struct Params {
    width: u32,
    height: u32,
    minimum: f32,
    extent: f32,
}
@group(0) @binding(0) var<storage, read> histogram: array<u32, 256>;
@group(0) @binding(1) var<uniform> params: Params;
@group(0) @binding(2) var<storage, read_write> result: array<u32, 3>;
var<workgroup> partial: array<u32, 256>;

@compute @workgroup_size(256)
fn reduce(@builtin(local_invocation_index) lane: u32) {
    partial[lane] = histogram[lane] * lane;
    workgroupBarrier();
    var stride = 128u;
    loop {
        if stride == 0u { break; }
        if lane < stride {
            partial[lane] = partial[lane] + partial[lane + stride];
        }
        workgroupBarrier();
        stride = stride >> 1u;
    }
    if lane == 0u {
        let denominator = max(f32(params.width * params.height), 1.0);
        let average = f32(partial[0]) / denominator;
        let normalized = average * bitcast<f32>(0x3b808081u);
        let exponent = normalized * params.extent + params.minimum;
        result[1] = bitcast<u32>(exp2(exponent));
    }
}

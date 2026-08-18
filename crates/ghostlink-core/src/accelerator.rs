//! Runtime-selected execution backend helpers.
//!
//! This module turns the detected acceleration mode into a concrete execution
//! profile with backend-specific chunk sizing and parallel slice execution.

use crate::host::{AccelerationMode, RuntimeProfile};
use std::mem::{ManuallyDrop, MaybeUninit};

#[derive(Copy, Clone)]
struct SendPtr(*mut f32);
unsafe impl Send for SendPtr {}
unsafe impl Sync for SendPtr {}

/// Concrete execution backend derived from the runtime profile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionBackend {
    /// Selected acceleration mode.
    pub mode: AccelerationMode,
    /// Number of worker slices used for execution.
    pub worker_count: usize,
    /// Preferred batch size per worker.
    pub preferred_batch_size: usize,
    /// Logical vector width used when chunking the inner loop.
    pub vector_width_bits: usize,
}

impl ExecutionBackend {
    /// Select an execution backend from the detected runtime profile.
    pub fn from_runtime_profile(profile: &RuntimeProfile) -> Self {
        let (preferred_batch_size, vector_width_bits) = match profile.acceleration_mode {
            AccelerationMode::Gpu => (4096, 512),
            AccelerationMode::Avx512 => (2048, 512),
            AccelerationMode::Avx2 => (1024, 256),
            AccelerationMode::Neon => (1024, 128),
            AccelerationMode::Generic => (512, 64),
        };

        Self {
            mode: profile.acceleration_mode,
            worker_count: profile.recommended_workers.max(1),
            preferred_batch_size,
            vector_width_bits,
        }
    }

    /// Human-readable backend name.
    pub const fn name(&self) -> &'static str {
        self.mode.as_str()
    }

    /// Scale a slice using the selected backend's worker and chunk sizing.
    pub fn scale_f32_slice(&self, input: &[f32], scale: f32) -> Vec<f32> {
        if input.is_empty() {
            return Vec::new();
        }

        // Allocate an uninitialized buffer of MaybeUninit<f32> to avoid zero-filling memset overhead.
        // SAFETY: MaybeUninit<f32> allows uninitialized memory, so calling set_len on Vec<MaybeUninit<f32>> is sound.
        let mut output: Vec<MaybeUninit<f32>> = Vec::with_capacity(input.len());
        unsafe {
            output.set_len(input.len());
        }

        let out_ptr = output.as_mut_ptr() as *mut f32;

        unsafe {
            match self.mode {
                AccelerationMode::Gpu => {
                    parallel_scale_scalar(input, out_ptr, scale, self.worker_count)
                }
                AccelerationMode::Avx512 => scale_x86_512(input, out_ptr, scale),
                AccelerationMode::Avx2 => scale_x86_256(input, out_ptr, scale),
                AccelerationMode::Neon => scale_neon(input, out_ptr, scale),
                AccelerationMode::Generic => scale_scalar(input, out_ptr, scale),
            }
        }

        // SAFETY: Every element in 0..input.len() has been fully initialized by the backend scaling function above.
        // Using ManuallyDrop + Vec::from_raw_parts avoids transmute between repr(Rust) Vec types.
        let mut md = ManuallyDrop::new(output);
        unsafe { Vec::from_raw_parts(md.as_mut_ptr() as *mut f32, md.len(), md.capacity()) }
    }
}

unsafe fn scale_scalar(input: &[f32], output: *mut f32, scale: f32) {
    for (i, &src) in input.iter().enumerate() {
        output.add(i).write(src * scale);
    }
}

unsafe fn parallel_scale_scalar(input: &[f32], output: *mut f32, scale: f32, worker_count: usize) {
    let worker_count = worker_count.max(1);
    // Thread fan-out can dominate runtime for moderate tensor sizes.
    // Keep GPU fallback scalar in-process unless there is enough work.
    const MIN_PARALLEL_LEN: usize = 65_536;
    if worker_count <= 1 || input.len() < MIN_PARALLEL_LEN {
        scale_scalar(input, output, scale);
        return;
    }

    let chunk_size = input.len().div_ceil(worker_count).max(1);
    std::thread::scope(|scope| {
        for (i, in_chunk) in input.chunks(chunk_size).enumerate() {
            let out_ptr = SendPtr(output.add(i * chunk_size));
            scope.spawn(move || {
                let p = out_ptr;
                scale_scalar(in_chunk, p.0, scale);
            });
        }
    });
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
unsafe fn scale_x86_256(input: &[f32], output: *mut f32, scale: f32) {
    if std::is_x86_feature_detected!("avx2") {
        scale_x86_256_impl(input, output, scale)
    } else {
        scale_scalar(input, output, scale)
    }
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
unsafe fn scale_x86_256(input: &[f32], output: *mut f32, scale: f32) {
    scale_scalar(input, output, scale)
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
unsafe fn scale_x86_512(input: &[f32], output: *mut f32, scale: f32) {
    // Keep AVX-512 mode API-compatible on stable Rust by falling back to AVX2/scalar.
    scale_x86_256(input, output, scale)
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
unsafe fn scale_x86_512(input: &[f32], output: *mut f32, scale: f32) {
    scale_scalar(input, output, scale)
}

#[cfg(target_arch = "aarch64")]
unsafe fn scale_neon(input: &[f32], output: *mut f32, scale: f32) {
    scale_neon_impl(input, output, scale)
}

#[cfg(not(target_arch = "aarch64"))]
unsafe fn scale_neon(input: &[f32], output: *mut f32, scale: f32) {
    scale_scalar(input, output, scale)
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn scale_x86_256_impl(input: &[f32], output: *mut f32, scale: f32) {
    use std::arch::x86_64::{_mm256_loadu_ps, _mm256_mul_ps, _mm256_set1_ps, _mm256_storeu_ps};

    let scale_vec = _mm256_set1_ps(scale);
    let mut index = 0usize;
    // Unroll 4x to process 32 floats (four 256-bit AVX2 registers) per loop iteration.
    // This allows modern CPU out-of-order execution pipelines to execute multiple independent
    // vector multiplications in parallel, avoiding dependency stalls and loop overhead.
    while index + 32 <= input.len() {
        let input_vec0 = _mm256_loadu_ps(input.as_ptr().add(index));
        let input_vec1 = _mm256_loadu_ps(input.as_ptr().add(index + 8));
        let input_vec2 = _mm256_loadu_ps(input.as_ptr().add(index + 16));
        let input_vec3 = _mm256_loadu_ps(input.as_ptr().add(index + 24));

        let output_vec0 = _mm256_mul_ps(input_vec0, scale_vec);
        let output_vec1 = _mm256_mul_ps(input_vec1, scale_vec);
        let output_vec2 = _mm256_mul_ps(input_vec2, scale_vec);
        let output_vec3 = _mm256_mul_ps(input_vec3, scale_vec);

        _mm256_storeu_ps(output.add(index), output_vec0);
        _mm256_storeu_ps(output.add(index + 8), output_vec1);
        _mm256_storeu_ps(output.add(index + 16), output_vec2);
        _mm256_storeu_ps(output.add(index + 24), output_vec3);

        index += 32;
    }
    // Clean up remaining vectors of size 8
    while index + 8 <= input.len() {
        let input_vec = _mm256_loadu_ps(input.as_ptr().add(index));
        let output_vec = _mm256_mul_ps(input_vec, scale_vec);
        _mm256_storeu_ps(output.add(index), output_vec);
        index += 8;
    }
    scale_scalar(&input[index..], output.add(index), scale);
}

#[cfg(target_arch = "x86")]
#[target_feature(enable = "avx2")]
unsafe fn scale_x86_256_impl(input: &[f32], output: *mut f32, scale: f32) {
    use std::arch::x86::{_mm256_loadu_ps, _mm256_mul_ps, _mm256_set1_ps, _mm256_storeu_ps};

    let scale_vec = _mm256_set1_ps(scale);
    let mut index = 0usize;
    // Unroll 4x to process 32 floats (four 256-bit AVX2 registers) per loop iteration.
    // This allows modern CPU out-of-order execution pipelines to execute multiple independent
    // vector multiplications in parallel, avoiding dependency stalls and loop overhead.
    while index + 32 <= input.len() {
        let input_vec0 = _mm256_loadu_ps(input.as_ptr().add(index));
        let input_vec1 = _mm256_loadu_ps(input.as_ptr().add(index + 8));
        let input_vec2 = _mm256_loadu_ps(input.as_ptr().add(index + 16));
        let input_vec3 = _mm256_loadu_ps(input.as_ptr().add(index + 24));

        let output_vec0 = _mm256_mul_ps(input_vec0, scale_vec);
        let output_vec1 = _mm256_mul_ps(input_vec1, scale_vec);
        let output_vec2 = _mm256_mul_ps(input_vec2, scale_vec);
        let output_vec3 = _mm256_mul_ps(input_vec3, scale_vec);

        _mm256_storeu_ps(output.add(index), output_vec0);
        _mm256_storeu_ps(output.add(index + 8), output_vec1);
        _mm256_storeu_ps(output.add(index + 16), output_vec2);
        _mm256_storeu_ps(output.add(index + 24), output_vec3);

        index += 32;
    }
    // Clean up remaining vectors of size 8
    while index + 8 <= input.len() {
        let input_vec = _mm256_loadu_ps(input.as_ptr().add(index));
        let output_vec = _mm256_mul_ps(input_vec, scale_vec);
        _mm256_storeu_ps(output.add(index), output_vec);
        index += 8;
    }
    scale_scalar(&input[index..], output.add(index), scale);
}

#[cfg(target_arch = "aarch64")]
unsafe fn scale_neon_impl(input: &[f32], output: *mut f32, scale: f32) {
    use std::arch::aarch64::{vdupq_n_f32, vld1q_f32, vmulq_f32, vst1q_f32};

    let scale_vec = vdupq_n_f32(scale);
    let mut index = 0usize;
    // Unroll 4x to process 16 floats (four 128-bit NEON registers) per loop iteration.
    // This allows modern CPU out-of-order execution pipelines to execute multiple independent
    // vector multiplications in parallel, avoiding dependency stalls and loop overhead.
    while index + 16 <= input.len() {
        let input_vec0 = vld1q_f32(input.as_ptr().add(index));
        let input_vec1 = vld1q_f32(input.as_ptr().add(index + 4));
        let input_vec2 = vld1q_f32(input.as_ptr().add(index + 8));
        let input_vec3 = vld1q_f32(input.as_ptr().add(index + 12));

        let output_vec0 = vmulq_f32(input_vec0, scale_vec);
        let output_vec1 = vmulq_f32(input_vec1, scale_vec);
        let output_vec2 = vmulq_f32(input_vec2, scale_vec);
        let output_vec3 = vmulq_f32(input_vec3, scale_vec);

        vst1q_f32(output.add(index), output_vec0);
        vst1q_f32(output.add(index + 4), output_vec1);
        vst1q_f32(output.add(index + 8), output_vec2);
        vst1q_f32(output.add(index + 12), output_vec3);

        index += 16;
    }
    // Clean up remaining vectors of size 4
    while index + 4 <= input.len() {
        let input_vec = vld1q_f32(input.as_ptr().add(index));
        let output_vec = vmulq_f32(input_vec, scale_vec);
        vst1q_f32(output.add(index), output_vec);
        index += 4;
    }
    scale_scalar(&input[index..], output.add(index), scale);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::NodeResources;

    #[test]
    fn backend_selection_reflects_acceleration_mode() {
        let profile = RuntimeProfile {
            node_resources: NodeResources::new("node-a", 24.0, 64.0, "8.9", None),
            logical_cores: 16,
            recommended_workers: 8,
            acceleration_mode: AccelerationMode::Gpu,
            gpu_backend: crate::host::GpuBackend::Cuda,
            xdp_supported: true,
            detection_source: String::from("test"),
            probe_mode: crate::host::ProbeMode::Fast,
        };

        let backend = ExecutionBackend::from_runtime_profile(&profile);
        assert_eq!(backend.name(), "GPU");
        assert_eq!(backend.vector_width_bits, 512);
    }

    #[test]
    fn backend_executes_scaled_transform() {
        let profile = RuntimeProfile {
            node_resources: NodeResources::new("node-a", 0.0, 64.0, "cpu", None),
            logical_cores: 8,
            recommended_workers: 4,
            acceleration_mode: AccelerationMode::Avx2,
            gpu_backend: crate::host::GpuBackend::Cpu,
            xdp_supported: true,
            detection_source: String::from("test"),
            probe_mode: crate::host::ProbeMode::Fast,
        };

        let backend = ExecutionBackend::from_runtime_profile(&profile);
        let output = backend.scale_f32_slice(&[1.0, 2.0, 3.5], 2.0);
        assert_eq!(output, vec![2.0, 4.0, 7.0]);
    }
}

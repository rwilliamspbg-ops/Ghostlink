//! Runtime-selected execution backend helpers.
//!
//! This module turns the detected acceleration mode into a concrete execution
//! profile with backend-specific chunk sizing and parallel slice execution.

use crate::host::{AccelerationMode, RuntimeProfile};

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

        let mut output = vec![0.0f32; input.len()];
        match self.mode {
            AccelerationMode::Gpu => {
                parallel_scale_scalar(input, &mut output, scale, self.worker_count)
            }
            AccelerationMode::Avx512 => scale_x86_512(input, &mut output, scale),
            AccelerationMode::Avx2 => scale_x86_256(input, &mut output, scale),
            AccelerationMode::Neon => scale_neon(input, &mut output, scale),
            AccelerationMode::Generic => scale_scalar(input, &mut output, scale),
        }

        output
    }
}

fn scale_scalar(input: &[f32], output: &mut [f32], scale: f32) {
    for (dst, src) in output.iter_mut().zip(input.iter()) {
        *dst = *src * scale;
    }
}

fn parallel_scale_scalar(input: &[f32], output: &mut [f32], scale: f32, worker_count: usize) {
    let chunk_size = input.len().div_ceil(worker_count.max(1)).max(1);
    std::thread::scope(|scope| {
        for (in_chunk, out_chunk) in input.chunks(chunk_size).zip(output.chunks_mut(chunk_size)) {
            scope.spawn(move || scale_scalar(in_chunk, out_chunk, scale));
        }
    });
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn scale_x86_256(input: &[f32], output: &mut [f32], scale: f32) {
    if std::is_x86_feature_detected!("avx2") {
        unsafe { scale_x86_256_impl(input, output, scale) }
    } else {
        scale_scalar(input, output, scale)
    }
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn scale_x86_256(input: &[f32], output: &mut [f32], scale: f32) {
    scale_scalar(input, output, scale)
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn scale_x86_512(input: &[f32], output: &mut [f32], scale: f32) {
    if std::is_x86_feature_detected!("avx512f") {
        unsafe { scale_x86_512_impl(input, output, scale) }
    } else {
        scale_x86_256(input, output, scale)
    }
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn scale_x86_512(input: &[f32], output: &mut [f32], scale: f32) {
    scale_scalar(input, output, scale)
}

#[cfg(target_arch = "aarch64")]
fn scale_neon(input: &[f32], output: &mut [f32], scale: f32) {
    unsafe { scale_neon_impl(input, output, scale) }
}

#[cfg(not(target_arch = "aarch64"))]
fn scale_neon(input: &[f32], output: &mut [f32], scale: f32) {
    scale_scalar(input, output, scale)
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn scale_x86_256_impl(input: &[f32], output: &mut [f32], scale: f32) {
    use std::arch::x86_64::{_mm256_loadu_ps, _mm256_mul_ps, _mm256_set1_ps, _mm256_storeu_ps};

    let scale_vec = _mm256_set1_ps(scale);
    let mut index = 0usize;
    while index + 8 <= input.len() {
        let input_vec = _mm256_loadu_ps(input.as_ptr().add(index));
        let output_vec = _mm256_mul_ps(input_vec, scale_vec);
        _mm256_storeu_ps(output.as_mut_ptr().add(index), output_vec);
        index += 8;
    }
    scale_scalar(&input[index..], &mut output[index..], scale);
}

#[cfg(target_arch = "x86")]
#[target_feature(enable = "avx2")]
unsafe fn scale_x86_256_impl(input: &[f32], output: &mut [f32], scale: f32) {
    use std::arch::x86::{_mm256_loadu_ps, _mm256_mul_ps, _mm256_set1_ps, _mm256_storeu_ps};

    let scale_vec = _mm256_set1_ps(scale);
    let mut index = 0usize;
    while index + 8 <= input.len() {
        let input_vec = _mm256_loadu_ps(input.as_ptr().add(index));
        let output_vec = _mm256_mul_ps(input_vec, scale_vec);
        _mm256_storeu_ps(output.as_mut_ptr().add(index), output_vec);
        index += 8;
    }
    scale_scalar(&input[index..], &mut output[index..], scale);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f")]
unsafe fn scale_x86_512_impl(input: &[f32], output: &mut [f32], scale: f32) {
    use std::arch::x86_64::{_mm512_loadu_ps, _mm512_mul_ps, _mm512_set1_ps, _mm512_storeu_ps};

    let scale_vec = _mm512_set1_ps(scale);
    let mut index = 0usize;
    while index + 16 <= input.len() {
        let input_vec = _mm512_loadu_ps(input.as_ptr().add(index));
        let output_vec = _mm512_mul_ps(input_vec, scale_vec);
        _mm512_storeu_ps(output.as_mut_ptr().add(index), output_vec);
        index += 16;
    }
    scale_scalar(&input[index..], &mut output[index..], scale);
}

#[cfg(target_arch = "x86")]
#[target_feature(enable = "avx512f")]
unsafe fn scale_x86_512_impl(input: &[f32], output: &mut [f32], scale: f32) {
    use std::arch::x86::{_mm512_loadu_ps, _mm512_mul_ps, _mm512_set1_ps, _mm512_storeu_ps};

    let scale_vec = _mm512_set1_ps(scale);
    let mut index = 0usize;
    while index + 16 <= input.len() {
        let input_vec = _mm512_loadu_ps(input.as_ptr().add(index));
        let output_vec = _mm512_mul_ps(input_vec, scale_vec);
        _mm512_storeu_ps(output.as_mut_ptr().add(index), output_vec);
        index += 16;
    }
    scale_scalar(&input[index..], &mut output[index..], scale);
}

#[cfg(target_arch = "aarch64")]
unsafe fn scale_neon_impl(input: &[f32], output: &mut [f32], scale: f32) {
    use std::arch::aarch64::{vdupq_n_f32, vld1q_f32, vmulq_f32, vst1q_f32};

    let scale_vec = vdupq_n_f32(scale);
    let mut index = 0usize;
    while index + 4 <= input.len() {
        let input_vec = vld1q_f32(input.as_ptr().add(index));
        let output_vec = vmulq_f32(input_vec, scale_vec);
        vst1q_f32(output.as_mut_ptr().add(index), output_vec);
        index += 4;
    }
    scale_scalar(&input[index..], &mut output[index..], scale);
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
            xdp_supported: true,
            detection_source: String::from("test"),
            probe_mode: crate::host::ProbeMode::Fast,
        };

        let backend = ExecutionBackend::from_runtime_profile(&profile);
        let output = backend.scale_f32_slice(&[1.0, 2.0, 3.5], 2.0);
        assert_eq!(output, vec![2.0, 4.0, 7.0]);
    }
}

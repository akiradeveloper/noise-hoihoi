//! Compute processor discovery and Burn runtime selection for `NoiseNet`.

use std::{
    collections::{HashMap, HashSet},
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{LazyLock, Mutex},
};

use burn::prelude::{Device, DeviceKind};
use sysinfo::{CpuRefreshKind, RefreshKind, System};
use thiserror::Error;
use wgpu::{AdapterInfo, Backends, DeviceType, Instance, InstanceDescriptor};

/// A processor on which `NoiseNet` can run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComputeProcessor {
    id: String,
    name: String,
    kind: ProcessorKind,
    selector: ProcessorSelector,
}

impl ComputeProcessor {
    /// Stable-enough identifier used to restore a persisted selection.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Human-readable processor name supplied by the operating system or driver.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Broad processor class used by the GUI.
    #[must_use]
    pub const fn kind(&self) -> ProcessorKind {
        self.kind
    }

    /// Whether this processor uses a GPU runtime.
    #[must_use]
    pub const fn is_gpu(&self) -> bool {
        self.kind.is_gpu()
    }

    /// Runtimes currently supported by this processor.
    #[must_use]
    pub fn runtimes(&self) -> &'static [ComputeRuntime] {
        if self.is_gpu() {
            &[ComputeRuntime::Wgpu]
        } else {
            &[ComputeRuntime::Flex]
        }
    }

    /// The only runtime available for this processor in `NoiseHoiHoi` v0.4.
    #[must_use]
    pub const fn default_runtime(&self) -> ComputeRuntime {
        if self.is_gpu() {
            ComputeRuntime::Wgpu
        } else {
            ComputeRuntime::Flex
        }
    }
}

/// Physical processor class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessorKind {
    Cpu,
    IntegratedGpu,
    DiscreteGpu,
    VirtualGpu,
}

impl ProcessorKind {
    #[must_use]
    pub const fn is_gpu(self) -> bool {
        !matches!(self, Self::Cpu)
    }
}

/// Inference runtime exposed by `NoiseNet`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComputeRuntime {
    Flex,
    Wgpu,
}

impl std::fmt::Display for ComputeRuntime {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Flex => formatter.write_str("Flex"),
            Self::Wgpu => formatter.write_str("WGPU"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ProcessorSelector {
    Cpu,
    Wgpu(DeviceKind),
}

/// Failure to construct the selected Burn device.
#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("{runtime} is not supported by processor '{processor}'")]
    UnsupportedSelection {
        processor: String,
        runtime: ComputeRuntime,
    },

    #[error("WGPU could not initialize processor '{0}'")]
    WgpuInitialization(String),
}

/// Enumerate the CPU and every selectable WGPU adapter on this system.
///
/// The CPU is always represented as one Flex processor. GPU discovery is
/// limited to DX12 on Windows and Vulkan elsewhere so the same physical GPU is
/// not returned once per graphics API.
#[must_use]
pub fn processors() -> Vec<ComputeProcessor> {
    let mut processors = vec![cpu()];
    let discovered = catch_unwind(AssertUnwindSafe(gpu_processors)).unwrap_or_default();
    processors.extend(discovered);
    processors
}

/// Return the host CPU as the single Flex processor.
#[must_use]
pub fn cpu() -> ComputeProcessor {
    cpu_processor()
}

/// Create the Burn device for a previously enumerated processor and runtime.
///
/// # Errors
///
/// Returns an error for an invalid processor/runtime pair or if WGPU fails to
/// initialize the selected adapter.
pub fn create_device(
    processor: &ComputeProcessor,
    runtime: ComputeRuntime,
) -> Result<Device, RuntimeError> {
    match (&processor.selector, runtime) {
        (ProcessorSelector::Cpu, ComputeRuntime::Flex) => Ok(Device::flex()),
        (ProcessorSelector::Wgpu(selector), ComputeRuntime::Wgpu) => {
            initialize_wgpu(selector)
                .map_err(|()| RuntimeError::WgpuInitialization(processor.name().to_owned()))?;
            Ok(Device::wgpu(selector.clone()))
        }
        _ => Err(RuntimeError::UnsupportedSelection {
            processor: processor.name().to_owned(),
            runtime,
        }),
    }
}

fn cpu_processor() -> ComputeProcessor {
    let system =
        System::new_with_specifics(RefreshKind::nothing().with_cpu(CpuRefreshKind::nothing()));
    let name = system
        .cpus()
        .iter()
        .map(sysinfo::Cpu::brand)
        .find(|brand| !brand.trim().is_empty())
        .map_or("CPU", str::trim)
        .to_owned();

    ComputeProcessor {
        id: "cpu".to_owned(),
        name,
        kind: ProcessorKind::Cpu,
        selector: ProcessorSelector::Cpu,
    }
}

fn gpu_processors() -> Vec<ComputeProcessor> {
    let backends = platform_backends();
    let instance = Instance::new(InstanceDescriptor {
        backends,
        ..InstanceDescriptor::new_without_display_handle()
    });
    let adapters = pollster::block_on(instance.enumerate_adapters(backends));
    let mut type_indices = HashMap::<DeviceType, usize>::new();
    let mut processors = adapters
        .into_iter()
        .filter_map(|adapter| {
            let info = adapter.get_info();
            let (kind, selector) = selector_for_adapter(&info, &mut type_indices)?;
            Some(ComputeProcessor {
                id: gpu_id(&info),
                name: info.name.trim().to_owned(),
                kind,
                selector: ProcessorSelector::Wgpu(selector),
            })
        })
        .collect::<Vec<_>>();
    disambiguate_duplicate_ids(&mut processors);
    processors.sort_by(|left, right| {
        processor_rank(left.kind)
            .cmp(&processor_rank(right.kind))
            .then_with(|| left.name.cmp(&right.name))
    });
    processors
}

fn selector_for_adapter(
    info: &AdapterInfo,
    type_indices: &mut HashMap<DeviceType, usize>,
) -> Option<(ProcessorKind, DeviceKind)> {
    let index = type_indices.entry(info.device_type).or_default();
    let result = match info.device_type {
        DeviceType::DiscreteGpu => (ProcessorKind::DiscreteGpu, DeviceKind::DiscreteGpu(*index)),
        DeviceType::IntegratedGpu => (
            ProcessorKind::IntegratedGpu,
            DeviceKind::IntegratedGpu(*index),
        ),
        DeviceType::VirtualGpu => (ProcessorKind::VirtualGpu, DeviceKind::VirtualGpu(*index)),
        DeviceType::Cpu | DeviceType::Other => return None,
    };
    *index += 1;
    Some(result)
}

fn gpu_id(info: &AdapterInfo) -> String {
    let name = info
        .name
        .trim()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    format!(
        "wgpu:{:?}:{:?}:{:08x}:{:08x}:{}:{}",
        info.backend,
        info.device_type,
        info.vendor,
        info.device,
        info.device_pci_bus_id.trim(),
        name
    )
}

fn disambiguate_duplicate_ids(processors: &mut [ComputeProcessor]) {
    let mut occurrences = HashMap::<String, usize>::new();
    for processor in processors {
        let occurrence = occurrences.entry(processor.id.clone()).or_default();
        if *occurrence != 0 {
            processor.id.push('#');
            processor.id.push_str(&occurrence.to_string());
            processor.name = format!("{} ({})", processor.name, *occurrence + 1);
        }
        *occurrence += 1;
    }
}

const fn processor_rank(kind: ProcessorKind) -> u8 {
    match kind {
        ProcessorKind::DiscreteGpu => 0,
        ProcessorKind::IntegratedGpu => 1,
        ProcessorKind::VirtualGpu => 2,
        ProcessorKind::Cpu => 3,
    }
}

#[cfg(windows)]
const fn platform_backends() -> Backends {
    Backends::DX12
}

#[cfg(not(windows))]
const fn platform_backends() -> Backends {
    Backends::VULKAN
}

fn initialize_wgpu(selector: &DeviceKind) -> Result<(), ()> {
    static INITIALIZED: LazyLock<Mutex<HashSet<DeviceKind>>> =
        LazyLock::new(|| Mutex::new(HashSet::new()));

    let mut initialized = INITIALIZED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if initialized.contains(selector) {
        return Ok(());
    }

    let device = to_wgpu_device(selector);
    let result = catch_unwind(AssertUnwindSafe(|| {
        #[cfg(windows)]
        burn_wgpu::init_setup::<burn_wgpu::graphics::Dx12>(
            &device,
            burn_wgpu::RuntimeOptions::default(),
        );
        #[cfg(not(windows))]
        burn_wgpu::init_setup::<burn_wgpu::graphics::Vulkan>(
            &device,
            burn_wgpu::RuntimeOptions::default(),
        );
    }));
    if result.is_err() {
        return Err(());
    }
    initialized.insert(selector.clone());
    Ok(())
}

fn to_wgpu_device(selector: &DeviceKind) -> burn_wgpu::WgpuDevice {
    match selector {
        DeviceKind::DiscreteGpu(index) => burn_wgpu::WgpuDevice::DiscreteGpu(*index),
        DeviceKind::IntegratedGpu(index) => burn_wgpu::WgpuDevice::IntegratedGpu(*index),
        DeviceKind::VirtualGpu(index) => burn_wgpu::WgpuDevice::VirtualGpu(*index),
        DeviceKind::Cpu => burn_wgpu::WgpuDevice::Cpu,
        DeviceKind::DefaultDevice => burn_wgpu::WgpuDevice::DefaultDevice,
        DeviceKind::Existing(id) => burn_wgpu::WgpuDevice::Existing(*id),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ComputeProcessor, ComputeRuntime, ProcessorKind, ProcessorSelector, create_device,
    };

    fn cpu() -> ComputeProcessor {
        ComputeProcessor {
            id: "cpu".to_owned(),
            name: "Test CPU".to_owned(),
            kind: ProcessorKind::Cpu,
            selector: ProcessorSelector::Cpu,
        }
    }

    fn gpu() -> ComputeProcessor {
        ComputeProcessor {
            id: "test-gpu".to_owned(),
            name: "Test GPU".to_owned(),
            kind: ProcessorKind::DiscreteGpu,
            selector: ProcessorSelector::Wgpu(burn::prelude::DeviceKind::DiscreteGpu(0)),
        }
    }

    #[test]
    fn cpu_exposes_only_flex() {
        let processor = cpu();
        assert_eq!(processor.runtimes(), &[ComputeRuntime::Flex]);
        assert_eq!(processor.default_runtime(), ComputeRuntime::Flex);
        assert!(!processor.is_gpu());
        assert!(create_device(&processor, ComputeRuntime::Flex).is_ok());
    }

    #[test]
    fn rejects_wgpu_on_the_flex_cpu() {
        assert!(create_device(&cpu(), ComputeRuntime::Wgpu).is_err());
    }

    #[test]
    fn gpu_exposes_only_wgpu() {
        let processor = gpu();
        assert_eq!(processor.runtimes(), &[ComputeRuntime::Wgpu]);
        assert_eq!(processor.default_runtime(), ComputeRuntime::Wgpu);
        assert!(processor.is_gpu());
    }
}

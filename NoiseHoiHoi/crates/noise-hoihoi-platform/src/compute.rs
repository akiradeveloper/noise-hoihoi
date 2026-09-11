use sysinfo::{CpuRefreshKind, RefreshKind, System};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComputeRuntime {
    Cpu,
    Vulkan,
}

impl std::fmt::Display for ComputeRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Cpu => "IREE CPU",
            Self::Vulkan => "IREE Vulkan",
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessorKind {
    Cpu,
    IntegratedGpu,
    DiscreteGpu,
}

#[derive(Clone, Debug)]
pub struct ComputeProcessor {
    id: String,
    name: String,
    kind: ProcessorKind,
}

impl ComputeProcessor {
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
    #[must_use]
    pub const fn kind(&self) -> ProcessorKind {
        self.kind
    }
    #[must_use]
    pub const fn is_gpu(&self) -> bool {
        !matches!(self.kind, ProcessorKind::Cpu)
    }
    #[must_use]
    pub const fn default_runtime(&self) -> ComputeRuntime {
        if self.is_gpu() {
            ComputeRuntime::Vulkan
        } else {
            ComputeRuntime::Cpu
        }
    }
}

pub fn processors() -> Vec<ComputeProcessor> {
    let system =
        System::new_with_specifics(RefreshKind::nothing().with_cpu(CpuRefreshKind::nothing()));
    let name = system
        .cpus()
        .iter()
        .map(sysinfo::Cpu::brand)
        .find(|s| !s.trim().is_empty())
        .unwrap_or("CPU");
    let mut result = vec![ComputeProcessor {
        id: "cpu".into(),
        name: name.trim().into(),
        kind: ProcessorKind::Cpu,
    }];
    // Missing GPU libraries or drivers must not hide CPU and passthrough.
    match noise_net_iree::devices() {
        Ok(devices) => result.extend(devices.into_iter().map(|device| ComputeProcessor {
            id: device.uri,
            name: device.name,
            kind: if device.integrated {
                ProcessorKind::IntegratedGpu
            } else {
                ProcessorKind::DiscreteGpu
            },
        })),
        Err(error) => eprintln!("GPU discovery: {error:#}"),
    }
    result
}

//! Experimental, read-only NUMA topology queries.
use crate::{MmapError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NumaNode(pub u32);

impl NumaNode {
    /// Samples the node of the CPU currently running this thread. The scheduler
    /// may migrate the thread immediately after the sample.
    pub fn current() -> Result<Self> {
        #[cfg(target_os = "linux")]
        {
            let cpu = rustix::thread::sched_getcpu();
            for entry in std::fs::read_dir(format!("/sys/devices/system/cpu/cpu{cpu}"))? {
                let name = entry?.file_name();
                if let Some(id) = name
                    .to_str()
                    .and_then(|n| n.strip_prefix("node"))
                    .and_then(|n| n.parse().ok())
                {
                    return Ok(Self(id));
                }
            }
        }
        Err(MmapError::UnsupportedPlatform)
    }
}

/// NUMA nodes exposed by the Linux kernel in sysfs.
pub struct NumaTopology {
    nodes: Vec<NumaNode>,
}
impl NumaTopology {
    pub fn detect() -> Result<Self> {
        #[cfg(target_os = "linux")]
        {
            let mut nodes = Vec::new();
            for entry in std::fs::read_dir("/sys/devices/system/node")? {
                let name = entry?.file_name();
                if let Some(id) = name
                    .to_str()
                    .and_then(|n| n.strip_prefix("node"))
                    .and_then(|n| n.parse().ok())
                {
                    nodes.push(NumaNode(id));
                }
            }
            if nodes.is_empty() {
                return Err(MmapError::UnsupportedPlatform);
            }
            nodes.sort_by_key(|n| n.0);
            Ok(Self { nodes })
        }
        #[cfg(not(target_os = "linux"))]
        {
            Err(MmapError::UnsupportedPlatform)
        }
    }
    pub fn nodes(&self) -> &[NumaNode] {
        &self.nodes
    }
    pub fn num_nodes(&self) -> u32 {
        self.nodes.len() as u32
    }
    pub fn is_available(&self) -> bool {
        self.nodes.len() > 1
    }
}

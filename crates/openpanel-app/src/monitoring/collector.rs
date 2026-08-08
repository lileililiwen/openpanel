//! Host metric collection. `SystemCollector` is the `sysinfo`-backed
//! adapter; the `Collector` trait lets tests inject a double.

use std::collections::HashMap;

use chrono::Utc;
use openpanel_domain::monitoring::{
    DiskReading, MonitoringError, NetworkReading, SystemSnapshot, clamp_to_percent,
};
use sysinfo::{Disks, Networks, System};

/// Port for producing a [`SystemSnapshot`] of the current host.
///
/// `&mut self` because a real collector advances cumulative counters
/// between calls. Implementations may be non-deterministic (real
/// `sysinfo`) or fixed (test doubles).
pub trait Collector: Send + 'static {
    /// Collect a fresh snapshot. The returned timestamp MUST NOT be in
    /// the future.
    fn snapshot(&mut self) -> Result<SystemSnapshot, MonitoringError>;
}

/// `sysinfo`-backed collector. Keeps cumulative network counters
/// between calls so it can report bytes-per-second deltas.
pub struct SystemCollector {
    system: System,
    disks: Disks,
    networks: Networks,
    /// Previous cumulative per-interface byte counters.
    prev_net: HashMap<String, (u64, u64)>,
}

impl SystemCollector {
    /// Create a collector and load the initial system state. The
    /// first `snapshot()` call establishes network baselines, so its
    /// throughput is `0` bps.
    pub fn new() -> Self {
        let mut system = System::new_all();
        system.refresh_all();
        let disks = Disks::new_with_refreshed_list();
        let networks = Networks::new_with_refreshed_list();
        Self {
            system,
            disks,
            networks,
            prev_net: HashMap::new(),
        }
    }

    /// Compute bytes-per-second deltas for every interface given the
    /// current cumulative counters. Updates the internal baseline.
    fn network_bps(&mut self) -> Vec<NetworkReading> {
        let mut out = Vec::new();
        for (name, data) in self.networks.iter() {
            let rx = data.received();
            let tx = data.transmitted();
            let (prev_rx, prev_tx) = self.prev_net.get(name).copied().unwrap_or((rx, tx));
            out.push(NetworkReading {
                interface: name.clone(),
                rx_bytes_per_sec: rx.saturating_sub(prev_rx),
                tx_bytes_per_sec: tx.saturating_sub(prev_tx),
            });
            self.prev_net.insert(name.clone(), (rx, tx));
        }
        out
    }
}

impl Default for SystemCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl Collector for SystemCollector {
    fn snapshot(&mut self) -> Result<SystemSnapshot, MonitoringError> {
        self.system.refresh_all();
        self.disks.refresh();
        self.networks.refresh();

        let cpu = clamp_to_percent(self.system.global_cpu_usage() as f64);
        let total_mem = self.system.total_memory();
        let used_mem = self.system.used_memory();
        let memory = if total_mem > 0 {
            clamp_to_percent((used_mem as f64 / total_mem as f64) * 100.0)
        } else {
            0.0
        };

        let disk: Vec<DiskReading> = self
            .disks
            .iter()
            .filter(|d| d.total_space() > 0)
            .map(|d| {
                let used = d.total_space().saturating_sub(d.available_space());
                DiskReading {
                    mount: d.mount_point().display().to_string(),
                    percent: clamp_to_percent((used as f64 / d.total_space() as f64) * 100.0),
                }
            })
            .collect();

        let network = self.network_bps();
        let load = System::load_average().one;

        SystemSnapshot::new(Utc::now(), load, cpu, memory, disk, network)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_values_are_sane() {
        let mut collector = SystemCollector::new();
        let snap = collector.snapshot().expect("collect");
        assert!(
            (0.0..=100.0).contains(&snap.cpu),
            "cpu {} out of [0,100]",
            snap.cpu
        );
        assert!(
            (0.0..=100.0).contains(&snap.memory),
            "memory {} out of [0,100]",
            snap.memory
        );
        assert!(
            snap.timestamp <= chrono::Utc::now(),
            "timestamp must not be in the future"
        );
        // Load average should be non-negative.
        assert!(snap.load >= 0.0);
        // At least one mount is reported on a real host.
        assert!(!snap.disk.is_empty(), "expected at least one disk");
    }
}

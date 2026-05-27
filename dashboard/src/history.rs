use crate::ipc::Snapshot;

/// Sparkline retention: ~4 minutes at the 1 s default refresh rate.
pub const HISTORY_CAPACITY: usize = 240;

/// A fixed-capacity ring buffer of `f32` samples. Once full, each `push`
/// overwrites the oldest sample.
#[derive(Debug, Clone)]
pub struct RingBuffer {
    data: Vec<f32>,
    capacity: usize,
    /// Index where the next sample will be written.
    head: usize,
    full: bool,
}

impl RingBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            data: Vec::with_capacity(capacity),
            capacity: capacity.max(1),
            head: 0,
            full: false,
        }
    }

    pub fn push(&mut self, value: f32) {
        if self.data.len() < self.capacity {
            self.data.push(value);
        } else {
            self.data[self.head] = value;
            self.full = true;
        }
        self.head = (self.head + 1) % self.capacity;
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Samples from oldest to newest.
    pub fn iter_oldest_first(&self) -> impl Iterator<Item = f32> + '_ {
        let (start, count) = if self.full {
            (self.head, self.capacity)
        } else {
            (0, self.data.len())
        };
        (0..count).map(move |i| self.data[(start + i) % self.capacity])
    }
}

impl Default for RingBuffer {
    fn default() -> Self {
        Self::new(HISTORY_CAPACITY)
    }
}

/// Per-GPU sparkline history: one buffer for compute load, one for memory
/// controller load.
#[derive(Debug, Default)]
pub struct GpuHistory {
    pub load: RingBuffer,
    pub memory_load: RingBuffer,
}

impl GpuHistory {
    pub fn new(capacity: usize) -> Self {
        Self {
            load: RingBuffer::new(capacity),
            memory_load: RingBuffer::new(capacity),
        }
    }
}

/// Network throughput history: one buffer for download, one for upload, both
/// in bytes per second.
#[derive(Debug, Default)]
pub struct NetThroughputHistory {
    pub down: RingBuffer,
    pub up: RingBuffer,
}

impl NetThroughputHistory {
    pub fn new(capacity: usize) -> Self {
        Self {
            down: RingBuffer::new(capacity),
            up: RingBuffer::new(capacity),
        }
    }
}

/// Per-component sparkline history. GPU buffers are indexed by GPU position in
/// the snapshot's `gpu` array.
#[derive(Debug, Default)]
pub struct History {
    pub cpu: Option<RingBuffer>,
    pub ram: Option<RingBuffer>,
    pub gpus: Vec<GpuHistory>,
    pub network: Option<NetThroughputHistory>,
}

impl History {
    /// Append one snapshot's load values to the matching buffers, creating
    /// buffers on first sight of a component.
    pub fn record(&mut self, snap: &Snapshot) {
        if let Some(cpu) = &snap.cpu {
            push_into(&mut self.cpu, cpu.load);
        }
        if let Some(ram) = &snap.ram {
            push_into(&mut self.ram, ram.load);
        }
        if let Some(gpus) = &snap.gpu {
            if self.gpus.len() != gpus.len() {
                self.gpus = (0..gpus.len())
                    .map(|_| GpuHistory::new(HISTORY_CAPACITY))
                    .collect();
            }
            for (buf, gpu) in self.gpus.iter_mut().zip(gpus) {
                buf.load.push(gpu.load.unwrap_or(0.0) as f32);
                buf.memory_load.push(gpu.memory_load.unwrap_or(0.0) as f32);
            }
        }
        if let Some(net) = &snap.net {
            let buf = self
                .network
                .get_or_insert_with(|| NetThroughputHistory::new(HISTORY_CAPACITY));
            buf.down.push(net.down_bps.unwrap_or(0.0) as f32);
            buf.up.push(net.up_bps.unwrap_or(0.0) as f32);
        }
    }
}

fn push_into(slot: &mut Option<RingBuffer>, value: Option<f64>) {
    slot.get_or_insert_with(|| RingBuffer::new(HISTORY_CAPACITY))
        .push(value.unwrap_or(0.0) as f32);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_only_the_last_capacity_samples() {
        let mut rb = RingBuffer::new(3);
        for v in [1.0, 2.0, 3.0, 4.0, 5.0] {
            rb.push(v);
        }
        let got: Vec<f32> = rb.iter_oldest_first().collect();
        assert_eq!(got, vec![3.0, 4.0, 5.0]);
        assert_eq!(rb.len(), 3);
    }

    #[test]
    fn iterates_oldest_to_newest_before_full() {
        let mut rb = RingBuffer::new(5);
        rb.push(10.0);
        rb.push(20.0);
        let got: Vec<f32> = rb.iter_oldest_first().collect();
        assert_eq!(got, vec![10.0, 20.0]);
    }

    #[test]
    fn empty_buffer_has_no_samples() {
        let rb = RingBuffer::new(4);
        assert_eq!(rb.len(), 0);
        assert_eq!(rb.iter_oldest_first().count(), 0);
    }

    #[test]
    fn network_buffer_records_both_directions() {
        let mut h = History::default();
        let snap = crate::ipc::Snapshot {
            v: 1,
            ts: 0,
            cpu: None,
            gpu: None,
            igpu: None,
            ram: None,
            storage: None,
            board: None,
            fans: None,
            voltages: None,
            net: Some(crate::ipc::NetInfo {
                adapter: "Ethernet".into(),
                down_bps: Some(1_000_000.0),
                up_bps: Some(250_000.0),
                link_bps: Some(1_000_000_000),
                down_pct: Some(0.1),
                up_pct: Some(0.025),
            }),
            battery: None,
            uptime_sec: None,
            atk_fans: None,
            display: None,
            displays: None,
            health: None,
        };
        h.record(&snap);
        h.record(&snap);
        let net = h.network.as_ref().expect("network history exists");
        let down: Vec<f32> = net.down.iter_oldest_first().collect();
        let up: Vec<f32> = net.up.iter_oldest_first().collect();
        assert_eq!(down, vec![1_000_000.0, 1_000_000.0]);
        assert_eq!(up, vec![250_000.0, 250_000.0]);
    }

    #[test]
    fn gpu_history_records_both_load_and_memory_load() {
        let mut h = History::default();
        let snap = crate::ipc::Snapshot {
            v: 1,
            ts: 0,
            cpu: None,
            gpu: Some(vec![crate::ipc::GpuInfo {
                name: "G".into(),
                kind: "discrete".into(),
                load: Some(40.0),
                temp: None,
                vram_used_mb: None,
                vram_total_mb: None,
                clock_mhz: None,
                fan_rpm: None,
                power_w: None,
                memory_load: Some(15.0),
                hot_spot_temp: None,
                memory_junction_temp_c: None,
                pcie_rx_bps: None,
                pcie_tx_bps: None,
                dedicated_vram_used_mb: None,
                shared_vram_used_mb: None,
                voltage_v: None,
                d3d_engines: None,
            }]),
            igpu: None,
            ram: None,
            storage: None,
            board: None,
            fans: None,
            voltages: None,
            net: None,
            battery: None,
            uptime_sec: None,
            atk_fans: None,
            display: None,
            displays: None,
            health: None,
        };
        h.record(&snap);
        h.record(&snap);
        let gpu = &h.gpus[0];
        let loads: Vec<f32> = gpu.load.iter_oldest_first().collect();
        let mem: Vec<f32> = gpu.memory_load.iter_oldest_first().collect();
        assert_eq!(loads, vec![40.0, 40.0]);
        assert_eq!(mem, vec![15.0, 15.0]);
    }
}

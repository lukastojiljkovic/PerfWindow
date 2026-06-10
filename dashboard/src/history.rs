use crate::ipc::Snapshot;
use std::collections::HashMap;

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

    /// Samples from oldest to newest. `ExactSizeIterator` so widgets can size
    /// their geometry without collecting into a `Vec` first.
    pub fn iter_oldest_first(&self) -> impl ExactSizeIterator<Item = f32> + '_ {
        let (start, count) = if self.full {
            (self.head, self.capacity)
        } else {
            (0, self.data.len())
        };
        ring_iter(&self.data, start, self.capacity, count)
    }
}

/// Shared iterator core of [`RingBuffer::iter_oldest_first`] and
/// [`samples_or_empty`] — both must yield the same opaque iterator type so an
/// absent buffer can degrade to an empty iteration without a `Vec` detour.
fn ring_iter(
    data: &[f32],
    start: usize,
    capacity: usize,
    count: usize,
) -> impl ExactSizeIterator<Item = f32> + '_ {
    (0..count).map(move |i| data[(start + i) % capacity])
}

/// Oldest-first samples of an optional buffer; `None` yields an empty
/// iterator. Lets panels hand sparkline widgets a uniform argument without a
/// per-frame `Vec` collection.
pub fn samples_or_empty(buf: Option<&RingBuffer>) -> impl ExactSizeIterator<Item = f32> + '_ {
    let (data, start, capacity, count) = match buf {
        Some(b) => {
            let (start, count) = if b.full {
                (b.head, b.capacity)
            } else {
                (0, b.data.len())
            };
            (b.data.as_slice(), start, b.capacity, count)
        }
        // `capacity` only feeds the modulo, which a zero count never reaches.
        None => (&[][..], 0, 1, 0),
    };
    ring_iter(data, start, capacity, count)
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

/// Per-component sparkline history.
///
/// GPU buffers are keyed by a name-derived key (duplicated names get an
/// occurrence suffix) so a transient GPU-count flicker — a driver hiccup
/// dropping a card from one snapshot — no longer wipes the history of the
/// GPUs that stayed. `gpu_order` mirrors the latest snapshot's `gpu` array so
/// panels keep addressing history by index via [`History::gpu`].
#[derive(Debug, Default)]
pub struct History {
    pub cpu: Option<RingBuffer>,
    pub ram: Option<RingBuffer>,
    gpus: HashMap<String, GpuHistory>,
    gpu_order: Vec<String>,
    pub network: Option<NetThroughputHistory>,
}

impl History {
    /// History for the GPU at `index` in the latest recorded snapshot's `gpu`
    /// array, or `None` before the first record (or for an out-of-range index).
    pub fn gpu(&self, index: usize) -> Option<&GpuHistory> {
        self.gpus.get(self.gpu_order.get(index)?)
    }

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
            self.gpu_order.clear();
            // Twin cards share a name; suffix repeats with their occurrence
            // index so each physical GPU keeps its own buffers.
            let mut seen: HashMap<&str, usize> = HashMap::new();
            for gpu in gpus {
                let occurrence = seen
                    .entry(gpu.name.as_str())
                    .and_modify(|c| *c += 1)
                    .or_insert(0);
                let key = if *occurrence == 0 {
                    gpu.name.clone()
                } else {
                    format!("{}#{}", gpu.name, occurrence)
                };
                let buf = self
                    .gpus
                    .entry(key.clone())
                    .or_insert_with(|| GpuHistory::new(HISTORY_CAPACITY));
                buf.load.push(gpu.load.unwrap_or(0.0) as f32);
                buf.memory_load.push(gpu.memory_load.unwrap_or(0.0) as f32);
                self.gpu_order.push(key);
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
                wifi: None,
            }),
            battery: None,
            uptime_sec: None,
            atk_fans: None,
            display: None,
            displays: None,
            health: None,
            ts_ms: None,
        };
        h.record(&snap);
        h.record(&snap);
        let net = h.network.as_ref().expect("network history exists");
        let down: Vec<f32> = net.down.iter_oldest_first().collect();
        let up: Vec<f32> = net.up.iter_oldest_first().collect();
        assert_eq!(down, vec![1_000_000.0, 1_000_000.0]);
        assert_eq!(up, vec![250_000.0, 250_000.0]);
    }

    /// Snapshot containing only a `gpu` array with one entry per `(name,
    /// load)` pair; `memory_load` is fixed at 15.0.
    fn gpu_snap(gpus: &[(&str, f64)]) -> crate::ipc::Snapshot {
        crate::ipc::Snapshot {
            v: 1,
            ts: 0,
            cpu: None,
            gpu: Some(
                gpus.iter()
                    .map(|&(name, load)| crate::ipc::GpuInfo {
                        name: name.into(),
                        kind: "discrete".into(),
                        load: Some(load),
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
                        memory_clock_mhz: None,
                        video_engine_load: None,
                    })
                    .collect(),
            ),
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
            ts_ms: None,
        }
    }

    #[test]
    fn gpu_history_records_both_load_and_memory_load() {
        let mut h = History::default();
        let snap = gpu_snap(&[("G", 40.0)]);
        h.record(&snap);
        h.record(&snap);
        let gpu = h.gpu(0).expect("gpu history exists");
        let loads: Vec<f32> = gpu.load.iter_oldest_first().collect();
        let mem: Vec<f32> = gpu.memory_load.iter_oldest_first().collect();
        assert_eq!(loads, vec![40.0, 40.0]);
        assert_eq!(mem, vec![15.0, 15.0]);
    }

    #[test]
    fn gpu_count_flicker_does_not_wipe_history() {
        let mut h = History::default();
        h.record(&gpu_snap(&[("A", 10.0), ("B", 20.0)]));
        // Transient enumeration glitch: B vanishes for one snapshot.
        h.record(&gpu_snap(&[("A", 11.0)]));
        h.record(&gpu_snap(&[("A", 12.0), ("B", 22.0)]));

        let a: Vec<f32> = h
            .gpu(0)
            .expect("A exists")
            .load
            .iter_oldest_first()
            .collect();
        let b: Vec<f32> = h
            .gpu(1)
            .expect("B exists")
            .load
            .iter_oldest_first()
            .collect();
        assert_eq!(a, vec![10.0, 11.0, 12.0]);
        // B's first sample survived the flicker; only the missing snapshot
        // is absent from its buffer.
        assert_eq!(b, vec![20.0, 22.0]);
    }

    #[test]
    fn twin_gpus_with_identical_names_keep_separate_buffers() {
        let mut h = History::default();
        h.record(&gpu_snap(&[("RTX 4090", 30.0), ("RTX 4090", 60.0)]));
        h.record(&gpu_snap(&[("RTX 4090", 31.0), ("RTX 4090", 61.0)]));
        let first: Vec<f32> = h.gpu(0).unwrap().load.iter_oldest_first().collect();
        let second: Vec<f32> = h.gpu(1).unwrap().load.iter_oldest_first().collect();
        assert_eq!(first, vec![30.0, 31.0]);
        assert_eq!(second, vec![60.0, 61.0]);
    }

    #[test]
    fn gpu_lookup_is_none_before_first_record_and_out_of_range() {
        let mut h = History::default();
        assert!(h.gpu(0).is_none());
        h.record(&gpu_snap(&[("A", 1.0)]));
        assert!(h.gpu(0).is_some());
        assert!(h.gpu(1).is_none());
    }

    #[test]
    fn samples_or_empty_yields_nothing_for_absent_history() {
        assert_eq!(samples_or_empty(None).len(), 0);
        let mut rb = RingBuffer::new(3);
        rb.push(1.0);
        rb.push(2.0);
        let got: Vec<f32> = samples_or_empty(Some(&rb)).collect();
        assert_eq!(got, vec![1.0, 2.0]);
    }

    #[test]
    fn samples_or_empty_matches_iter_oldest_first_when_full() {
        let mut rb = RingBuffer::new(3);
        for v in [1.0, 2.0, 3.0, 4.0, 5.0] {
            rb.push(v);
        }
        let a: Vec<f32> = rb.iter_oldest_first().collect();
        let b: Vec<f32> = samples_or_empty(Some(&rb)).collect();
        assert_eq!(a, b);
    }
}

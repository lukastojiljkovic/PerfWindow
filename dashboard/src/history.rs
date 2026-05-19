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

/// Per-component sparkline history. GPU buffers are indexed by GPU position in
/// the snapshot's `gpu` array.
#[derive(Debug, Default)]
pub struct History {
    pub cpu: Option<RingBuffer>,
    pub ram: Option<RingBuffer>,
    pub gpus: Vec<RingBuffer>,
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
                    .map(|_| RingBuffer::new(HISTORY_CAPACITY))
                    .collect();
            }
            for (buf, gpu) in self.gpus.iter_mut().zip(gpus) {
                buf.push(gpu.load.unwrap_or(0.0) as f32);
            }
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
}

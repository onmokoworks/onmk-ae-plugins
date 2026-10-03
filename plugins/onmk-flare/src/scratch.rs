//! Device-local, single-frame scratch reuse. Never share a slot across passes
//! that need distinct contents. The caller must wait for completion before reuse.
use std::collections::HashMap;
use wgpu::*;

#[derive(Default)]
pub(crate) struct Scratch {
    dimensions: (usize, usize),
    buffers: HashMap<&'static str, (Buffer, bool)>,
    #[cfg(test)]
    pub allocations: usize,
}
impl Scratch {
    pub fn begin(&mut self, w: usize, h: usize) {
        if self.dimensions != (w, h) {
            self.clear();
        }
        self.dimensions = (w, h);
        for (_, used) in self.buffers.values_mut() {
            *used = false;
        }
    }
    pub fn finish(&mut self) {
        self.buffers.retain(|_, (_, used)| *used);
    }
    pub fn clear(&mut self) {
        self.buffers.clear();
        self.dimensions = (0, 0);
    }
    pub fn bytes(&self) -> u64 {
        self.buffers.values().map(|(b, _)| b.size()).sum()
    }
    pub fn buffer(
        &mut self,
        device: &Device,
        slot: &'static str,
        size: u64,
        usage: BufferUsages,
    ) -> Buffer {
        if let Some((buffer, used)) = self.buffers.get_mut(slot) {
            if buffer.size() == size && buffer.usage() == usage {
                *used = true;
                return buffer.clone();
            }
        }
        self.buffers.remove(slot);
        let buffer = device.create_buffer(&BufferDescriptor {
            label: Some(slot),
            size,
            usage,
            mapped_at_creation: false,
        });
        // Over-budget buffers are invocation-local, never retained.
        if self.bytes().saturating_add(size) <= 256 * 1024 * 1024 {
            self.buffers.insert(slot, (buffer.clone(), true));
        }
        #[cfg(test)]
        {
            self.allocations += 1;
        }
        buffer
    }
}

pub(crate) fn buffer(
    pool: Option<&mut Scratch>,
    device: &Device,
    slot: &'static str,
    size: u64,
    usage: BufferUsages,
) -> Buffer {
    match pool {
        Some(pool) => pool.buffer(device, slot, size, usage),
        None => device.create_buffer(&BufferDescriptor {
            label: Some(slot),
            size,
            usage,
            mapped_at_creation: false,
        }),
    }
}

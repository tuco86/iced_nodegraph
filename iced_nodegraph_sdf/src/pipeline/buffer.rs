//! GPU buffer wrapper with dynamic resizing.

#![allow(dead_code)]

use encase::{ShaderSize, ShaderType, internal::WriteInto};
use iced::wgpu::{self, BindingResource};

const BUFFER_GROWTH_FACTOR: f32 = 1.5;
const BUFFER_MIN_ITEMS: usize = 16;

/// GPU buffer wrapper with incremental update support.
///
/// Manages a GPU storage buffer alongside a CPU-side Vec mirror. The GPU buffer
/// grows like a Vec (factor 1.5x) and is never shrunk, so steady-state frames
/// after the first few cause zero GPU allocations.
///
/// PERSISTENT write model (idle-skip groundwork): `clear()` only rewinds the live
/// length to 0; it does NOT drop the CPU mirror or the GPU data. Each frame's
/// writes OVERWRITE from slot 0 via `push`/`push_bulk`, so the previous frame's
/// contents survive in `buffer_vec` for change detection and a skipped write
/// leaves valid data in place. `live_len` is the count written this frame; slots
/// past it are stale but never read (consumers bound their reads by `len()`),
/// which also makes a shrinking frame truncate for free.
pub(crate) struct Buffer<T> {
    buffer_wgpu: wgpu::Buffer,
    /// CPU mirror. May hold MORE than `live_len` items (the high-water mark of any
    /// frame); only `[..live_len]` is live this frame.
    buffer_vec: Vec<T>,
    /// Items written this frame (the logical length). `len()` returns this.
    live_len: usize,
    scratch: Vec<u8>,
    label: Option<&'static str>,
    usage: wgpu::BufferUsages,
    generation: u64,
}

impl<T: ShaderSize> Buffer<T> {
    pub fn new(
        device: &wgpu::Device,
        label: Option<&'static str>,
        usage: wgpu::BufferUsages,
    ) -> Self {
        let item_size = T::SHADER_SIZE.get() as usize;
        let size = (BUFFER_MIN_ITEMS * item_size) as wgpu::BufferAddress;
        let buffer_wgpu = create_wgpu_buffer(device, label, size, usage);
        let buffer_vec = Vec::with_capacity(BUFFER_MIN_ITEMS);
        Self {
            buffer_wgpu,
            buffer_vec,
            live_len: 0,
            scratch: Vec::new(),
            label,
            usage,
            generation: 0,
        }
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn as_entire_binding(&self) -> BindingResource<'_> {
        self.buffer_wgpu.as_entire_binding()
    }

    /// The underlying GPU buffer (e.g. as a `copy_buffer_to_buffer` source).
    pub fn wgpu_buffer(&self) -> &wgpu::Buffer {
        &self.buffer_wgpu
    }

    pub fn len(&self) -> usize {
        self.live_len
    }

    /// The CPU-side mirror of the buffer's LIVE contents this frame (exactly what
    /// was uploaded). Lets a test inspect the data without a GPU readback. Excludes
    /// stale slots past `live_len`.
    pub fn cpu_mirror(&self) -> &[T] {
        &self.buffer_vec[..self.live_len]
    }

    pub fn is_empty(&self) -> bool {
        self.live_len == 0
    }

    #[must_use]
    pub fn push(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, item: T) -> usize
    where
        T: ShaderType + ShaderSize + WriteInto,
    {
        let slot = self.live_len;
        if slot < self.buffer_vec.len() {
            self.buffer_vec[slot] = item;
        } else {
            self.buffer_vec.push(item);
        }
        self.live_len += 1;

        let item_size = T::SHADER_SIZE.get() as usize;
        let offset = slot * item_size;
        let required_size = self.live_len * item_size;

        if self.buffer_wgpu.size() < required_size as u64 {
            let new_size = ((required_size as f32 * BUFFER_GROWTH_FACTOR) as u64)
                .max((BUFFER_MIN_ITEMS * T::SHADER_SIZE.get() as usize) as u64);
            self.buffer_wgpu = create_wgpu_buffer(device, self.label, new_size, self.usage);
            self.generation += 1;
            self.rewrite_all(queue);
        } else {
            self.scratch.clear();
            self.scratch.resize(item_size, 0);
            let mut writer = encase::StorageBuffer::new(&mut self.scratch[..]);
            writer
                .write(&self.buffer_vec[slot])
                .expect("Failed to write to storage buffer");
            queue.write_buffer(&self.buffer_wgpu, offset as u64, &self.scratch);
        }
        slot
    }

    fn rewrite_all(&mut self, queue: &wgpu::Queue)
    where
        T: ShaderType + ShaderSize + WriteInto,
    {
        if self.live_len == 0 {
            return;
        }

        let item_size = T::SHADER_SIZE.get() as usize;
        let total_size = self.live_len * item_size;
        self.scratch.clear();
        self.scratch.resize(total_size, 0);

        for (i, item) in self.buffer_vec[..self.live_len].iter().enumerate() {
            let offset = i * item_size;
            let slice = &mut self.scratch[offset..offset + item_size];
            let mut writer = encase::StorageBuffer::new(slice);
            writer
                .write(item)
                .expect("Failed to write to storage buffer");
        }
        queue.write_buffer(&self.buffer_wgpu, 0, &self.scratch);
    }

    #[must_use]
    pub fn push_bulk(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, items: &[T]) -> usize
    where
        T: ShaderType + ShaderSize + WriteInto + Clone,
    {
        if items.is_empty() {
            return self.live_len;
        }

        let start_slot = self.live_len;
        // Overwrite the live slots in place; extend the mirror only past its
        // high-water mark so prior-frame allocation is reused, not regrown.
        let overwrite = items
            .len()
            .min(self.buffer_vec.len().saturating_sub(start_slot));
        self.buffer_vec[start_slot..start_slot + overwrite].clone_from_slice(&items[..overwrite]);
        if overwrite < items.len() {
            self.buffer_vec.extend_from_slice(&items[overwrite..]);
        }
        self.live_len = start_slot + items.len();

        let item_size = T::SHADER_SIZE.get() as usize;
        let required_size = self.live_len * item_size;

        if self.buffer_wgpu.size() < required_size as u64 {
            let new_size = ((required_size as f32 * BUFFER_GROWTH_FACTOR) as u64)
                .max((BUFFER_MIN_ITEMS * T::SHADER_SIZE.get() as usize) as u64);
            self.buffer_wgpu = create_wgpu_buffer(device, self.label, new_size, self.usage);
            self.generation += 1;
            self.rewrite_all(queue);
        } else {
            let total_write = items.len() * item_size;
            let offset = start_slot * item_size;
            self.scratch.clear();
            self.scratch.resize(total_write, 0);

            for (i, item) in items.iter().enumerate() {
                let item_offset = i * item_size;
                let slice = &mut self.scratch[item_offset..item_offset + item_size];
                let mut writer = encase::StorageBuffer::new(slice);
                writer
                    .write(item)
                    .expect("Failed to write to storage buffer");
            }
            queue.write_buffer(&self.buffer_wgpu, offset as u64, &self.scratch);
        }
        start_slot
    }

    /// Overwrite the item at an existing `index` in place - one slot, CPU mirror
    /// and GPU buffer, no growth and no full rewrite. This is the basis for
    /// incremental command updates (R2): dragging one node rewrites ONE command
    /// instead of the whole buffer, so update cost scales with nodes-moved, not
    /// nodes-total. Panics if `index` is out of bounds.
    pub fn write_at(&mut self, queue: &wgpu::Queue, index: usize, item: T)
    where
        T: ShaderType + ShaderSize + WriteInto,
    {
        assert!(
            index < self.live_len,
            "write_at index {index} out of bounds (live len {})",
            self.live_len,
        );
        let item_size = T::SHADER_SIZE.get() as usize;
        self.buffer_vec[index] = item;
        self.scratch.clear();
        self.scratch.resize(item_size, 0);
        let mut writer = encase::StorageBuffer::new(&mut self.scratch[..]);
        writer
            .write(&self.buffer_vec[index])
            .expect("Failed to write to storage buffer");
        queue.write_buffer(&self.buffer_wgpu, (index * item_size) as u64, &self.scratch);
    }

    /// Rewinds the live length to 0 WITHOUT dropping the CPU mirror or GPU data,
    /// so next frame's writes overwrite in place and unwritten slots keep their
    /// previous contents (the basis for skipping unchanged writes).
    pub fn clear(&mut self) {
        self.live_len = 0;
    }
}

fn create_wgpu_buffer(
    device: &wgpu::Device,
    label: Option<&str>,
    size: wgpu::BufferAddress,
    usage: wgpu::BufferUsages,
) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label,
        size,
        usage,
        mapped_at_creation: false,
    })
}

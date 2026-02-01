//! GPU buffer wrapper with dynamic resizing.
//!
//! Adapted from iced_nodegraph's buffer implementation.

use encase::{ShaderSize, ShaderType, internal::WriteInto};
use iced::wgpu::{self, BindingResource};

/// Growth factor for buffer capacity to reduce reallocations.
const BUFFER_GROWTH_FACTOR: f32 = 1.5;

/// Minimum initial buffer capacity.
const BUFFER_MIN_CAPACITY: usize = 16;

/// GPU buffer wrapper with incremental update support.
///
/// Tracks a generation counter that increments when the underlying GPU buffer
/// is recreated, enabling bind group caching.
pub struct Buffer<T> {
    buffer_wgpu: wgpu::Buffer,
    buffer_vec: Vec<T>,
    /// Scratch buffer for encase serialization
    scratch: Vec<u8>,
    label: Option<&'static str>,
    usage: wgpu::BufferUsages,
    /// Generation counter - increments when buffer is recreated.
    generation: u64,
}

impl<T> Buffer<T> {
    pub fn new(
        device: &wgpu::Device,
        label: Option<&'static str>,
        usage: wgpu::BufferUsages,
    ) -> Self {
        let capacity = BUFFER_MIN_CAPACITY;
        let size = capacity as wgpu::BufferAddress * 256; // Conservative estimate
        let buffer_wgpu = create_wgpu_buffer(device, label, size, usage);
        let buffer_vec = Vec::with_capacity(capacity);
        Self {
            buffer_wgpu,
            buffer_vec,
            scratch: Vec::new(),
            label,
            usage,
            generation: 0,
        }
    }

    /// Returns the generation counter.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn as_entire_binding(&self) -> BindingResource<'_> {
        self.buffer_wgpu.as_entire_binding()
    }

    pub fn len(&self) -> usize {
        self.buffer_vec.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buffer_vec.is_empty()
    }

    /// Push a single item to the buffer and write it to GPU.
    #[must_use]
    pub fn push(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, item: T) -> usize
    where
        T: ShaderType + ShaderSize + WriteInto,
    {
        let slot = self.buffer_vec.len();
        self.buffer_vec.push(item);

        let item_size = T::SHADER_SIZE.get() as usize;
        let offset = slot * item_size;
        let required_size = (slot + 1) * item_size;

        if self.buffer_wgpu.size() < required_size as u64 {
            let new_size = ((required_size as f32 * BUFFER_GROWTH_FACTOR) as u64)
                .max(BUFFER_MIN_CAPACITY as u64 * 256);
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

    /// Rewrite entire buffer to GPU.
    fn rewrite_all(&mut self, queue: &wgpu::Queue)
    where
        T: ShaderType + ShaderSize + WriteInto,
    {
        if self.buffer_vec.is_empty() {
            return;
        }

        let item_size = T::SHADER_SIZE.get() as usize;
        let total_size = self.buffer_vec.len() * item_size;

        self.scratch.clear();
        self.scratch.resize(total_size, 0);

        for (i, item) in self.buffer_vec.iter().enumerate() {
            let offset = i * item_size;
            let slice = &mut self.scratch[offset..offset + item_size];
            let mut writer = encase::StorageBuffer::new(slice);
            writer
                .write(item)
                .expect("Failed to write to storage buffer");
        }

        queue.write_buffer(&self.buffer_wgpu, 0, &self.scratch);
    }

    /// Clear the buffer for next frame.
    pub fn clear(&mut self) {
        self.buffer_vec.clear();
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

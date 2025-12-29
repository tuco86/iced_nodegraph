use encase::{ShaderSize, ShaderType, internal::WriteInto};
use iced::wgpu::{self, BindingResource};

/// Growth factor for buffer capacity to reduce reallocations.
const BUFFER_GROWTH_FACTOR: f32 = 1.5;

/// Minimum initial buffer capacity.
const BUFFER_MIN_CAPACITY: usize = 16;

/// GPU buffer wrapper with dirty tracking and incremental update support.
///
/// Tracks a generation counter that increments when the underlying GPU buffer
/// is recreated, enabling bind group caching.
///
/// Also tracks content hash to skip redundant GPU writes on WebGPU/WASM where
/// excessive staging buffer usage can cause memory exhaustion.
pub struct Buffer<T> {
    buffer_wgpu: wgpu::Buffer,
    buffer_vec: Vec<T>,
    /// Scratch buffer for encase serialization
    scratch: Vec<u8>,
    label: Option<&'static str>,
    usage: wgpu::BufferUsages,
    /// Generation counter - increments when buffer is recreated.
    /// Used for bind group caching.
    generation: u64,
    /// Hash of last written content to skip redundant writes.
    /// Critical for WebGPU/WASM stability.
    content_hash: u64,
}

impl<T> Buffer<T> {
    pub fn new(
        device: &wgpu::Device,
        label: Option<&'static str>,
        usage: wgpu::BufferUsages,
    ) -> Self {
        let capacity = BUFFER_MIN_CAPACITY;
        // Use a reasonable initial size estimate (items don't have ShaderSize here)
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
            content_hash: 0,
        }
    }

    /// Returns the generation counter.
    ///
    /// This value increments every time the underlying GPU buffer is recreated.
    /// Use this to determine if bind groups need to be recreated.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Full update from iterator (original behavior, for compatibility).
    ///
    /// Returns the number of items written.
    #[must_use]
    pub fn update<I: IntoIterator<Item = T>>(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        data: I,
    ) -> u32
    where
        T: ShaderType + ShaderSize + WriteInto,
    {
        self.buffer_vec.clear();
        self.buffer_vec.extend(data);

        self.ensure_capacity_and_write_all(device, queue);
        self.buffer_vec.len() as _
    }

    /// Ensure buffer has capacity and write all data.
    ///
    /// Returns true if buffer was recreated.
    fn ensure_capacity_and_write_all(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> bool
    where
        T: ShaderType + ShaderSize + WriteInto,
    {
        if self.buffer_vec.is_empty() {
            return false;
        }

        // Calculate total size needed: each item takes SHADER_SIZE bytes
        let item_size = T::SHADER_SIZE.get() as usize;
        let total_size = self.buffer_vec.len() * item_size;

        // Resize scratch buffer and zero it
        self.scratch.clear();
        self.scratch.resize(total_size, 0);

        // Write each item at its correct offset using encase
        for (i, item) in self.buffer_vec.iter().enumerate() {
            let offset = i * item_size;
            let slice = &mut self.scratch[offset..offset + item_size];
            // Use encase's StorageBuffer to write a single item into the slice
            let mut writer = encase::StorageBuffer::new(slice);
            writer
                .write(item)
                .expect("Failed to write to storage buffer");
        }

        let bytes = &self.scratch[..];

        let required_size = bytes.len() as wgpu::BufferAddress;

        let recreated = if self.buffer_wgpu.size() < required_size {
            // Need larger buffer - grow with headroom
            let new_size = ((required_size as f32 * BUFFER_GROWTH_FACTOR) as u64)
                .max(BUFFER_MIN_CAPACITY as u64 * 256);

            self.buffer_wgpu = create_wgpu_buffer(device, self.label, new_size, self.usage);
            self.generation += 1;
            // Force write after buffer recreation
            self.content_hash = 0;
            true
        } else {
            false
        };

        // Compute content hash to avoid redundant writes.
        // Critical for WebGPU/WASM where staging buffer exhaustion causes crashes.
        let new_hash = simple_hash(bytes);

        if new_hash != self.content_hash || recreated {
            queue.write_buffer(&self.buffer_wgpu, 0, bytes);
            self.content_hash = new_hash;
        }

        recreated
    }

    pub fn as_entire_binding(&self) -> BindingResource<'_> {
        self.buffer_wgpu.as_entire_binding()
    }

    pub fn len(&self) -> usize {
        self.buffer_vec.len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.buffer_vec.is_empty()
    }
}

/// Fast, simple hash function for content change detection.
/// Uses FNV-1a algorithm for speed - we don't need cryptographic strength.
fn simple_hash(bytes: &[u8]) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut hash = FNV_OFFSET;
    for &byte in bytes {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
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

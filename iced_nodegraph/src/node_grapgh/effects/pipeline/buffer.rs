use bytemuck::NoUninit;
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
        let size =
            capacity as wgpu::BufferAddress * std::mem::size_of::<T>() as wgpu::BufferAddress;
        let buffer_wgpu = create_wgpu_buffer(device, label, size, usage);
        let buffer_vec = Vec::with_capacity(capacity);
        Self {
            buffer_wgpu,
            buffer_vec,
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
        T: NoUninit,
    {
        self.buffer_vec.clear();
        self.buffer_vec.extend(data);

        self.ensure_capacity_and_write_all(device, queue);
        self.buffer_vec.len() as _
    }

    /// Incremental update with dirty tracking.
    ///
    /// Only uploads changed items when structure hasn't changed.
    /// Falls back to full update on structural changes.
    ///
    /// # Arguments
    /// * `new_data` - Complete new data (needed for structural changes)
    /// * `dirty_indices` - Iterator of indices that changed
    /// * `structure_changed` - If true, forces full rebuild
    ///
    /// # Returns
    /// * `(count, buffer_recreated)` - Number of items and whether buffer was recreated
    #[must_use]
    pub fn update_incremental(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        new_data: &[T],
        dirty_indices: impl Iterator<Item = usize>,
        structure_changed: bool,
    ) -> (u32, bool)
    where
        T: NoUninit + Clone,
    {
        // If structure changed or length differs, do full update
        if structure_changed || new_data.len() != self.buffer_vec.len() {
            self.buffer_vec.clear();
            self.buffer_vec.extend_from_slice(new_data);
            let recreated = self.ensure_capacity_and_write_all(device, queue);
            return (self.buffer_vec.len() as u32, recreated);
        }

        // Incremental update: only write dirty items
        let item_size = std::mem::size_of::<T>();
        let mut any_written = false;

        for idx in dirty_indices {
            if idx < new_data.len() {
                // Update local copy
                self.buffer_vec[idx] = new_data[idx].clone();

                // Write to GPU at specific offset
                let offset = (idx * item_size) as wgpu::BufferAddress;
                queue.write_buffer(
                    &self.buffer_wgpu,
                    offset,
                    bytemuck::bytes_of(&new_data[idx]),
                );
                any_written = true;
            }
        }

        // If nothing was written but we were called, ensure buffer_vec is synced
        if !any_written && !new_data.is_empty() {
            // Data is already synced from previous full updates
        }

        (self.buffer_vec.len() as u32, false)
    }

    /// Ensure buffer has capacity and write all data.
    ///
    /// Returns true if buffer was recreated.
    fn ensure_capacity_and_write_all(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> bool
    where
        T: NoUninit,
    {
        let required_size = self.buffer_vec.capacity() as wgpu::BufferAddress
            * std::mem::size_of::<T>() as wgpu::BufferAddress;

        let recreated = if self.buffer_wgpu.size() < required_size {
            // Need larger buffer - grow with headroom
            let new_capacity = ((self.buffer_vec.capacity() as f32 * BUFFER_GROWTH_FACTOR)
                as usize)
                .max(BUFFER_MIN_CAPACITY);
            let new_size = new_capacity as wgpu::BufferAddress
                * std::mem::size_of::<T>() as wgpu::BufferAddress;

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
        if !self.buffer_vec.is_empty() {
            let bytes = bytemuck::cast_slice::<T, u8>(&self.buffer_vec);
            let new_hash = simple_hash(bytes);

            if new_hash != self.content_hash || recreated {
                queue.write_buffer(&self.buffer_wgpu, 0, bytes);
                self.content_hash = new_hash;
            }
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

use bytemuck::NoUninit;
use iced::wgpu::{self, BindingResource};

// mod uniforms;
// mod nodes;
// mod pins;
// mod edges;

pub struct Buffer<T> {
    buffer_wgpu: wgpu::Buffer,
    buffer_vec: Vec<T>,
    label: Option<&'static str>,
    usage: wgpu::BufferUsages,
}

impl<T> Buffer<T> {
    pub fn new(
        device: &wgpu::Device,
        label: Option<&'static str>,
        usage: wgpu::BufferUsages,
    ) -> Self {
        let size = 10 as wgpu::BufferAddress * std::mem::size_of::<T>() as wgpu::BufferAddress;
        let buffer_wgpu = create_wgpu_buffer(device, label, size, usage);
        let buffer_vec = Vec::with_capacity(10);
        Self {
            buffer_wgpu,
            buffer_vec,
            label,
            usage,
        }
    }

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

        let size = self.buffer_vec.capacity() as wgpu::BufferAddress
            * std::mem::size_of::<T>() as wgpu::BufferAddress;
        if self.buffer_wgpu.size() != size {
            self.buffer_wgpu = create_wgpu_buffer(device, self.label, size, self.usage);
        }
        queue.write_buffer(&self.buffer_wgpu, 0, bytemuck::cast_slice(&self.buffer_vec));

        self.buffer_vec.len() as _
    }

    pub fn as_entire_binding(&self) -> BindingResource<'_> {
        self.buffer_wgpu.as_entire_binding()
    }

    pub fn len(&self) -> usize {
        self.buffer_vec.len()
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

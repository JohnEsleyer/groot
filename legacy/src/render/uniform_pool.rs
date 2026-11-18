//! Pooled per-model uniform buffers.
//!
//! Every visible entity needs its own model matrix and colour in a uniform
//! buffer. Creating a fresh GPU buffer *and* bind group for every entity on
//! every frame churns the GPU allocator and produces a lot of driver
//! overhead, so the buffers are pooled: the pool owns one slot per draw
//! call, the slot's buffer is created once and only its contents are
//! rewritten on the following frames.

use std::rc::Rc;

use wgpu::util::DeviceExt;

/// The per-object uniform block written by the 2D and 3D model shaders.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ModelUniform {
    pub model: [[f32; 4]; 4],
    pub color: [f32; 4],
}

/// A reusable pool of model uniform buffers and their bind groups.
///
/// Usage per frame:
///
/// ```ignore
/// pool.reset();                       // start of the frame
/// let bind_group = pool.acquire(&device, &queue, &layout, matrix, color);
/// ```
///
/// Slots are handed out in order and reused across frames. A slot's buffer is
/// created the first time it is needed; afterwards the uniform data is
/// written into the existing buffer with `queue.write_buffer`.
pub struct ModelUniformPool {
    buffers: Vec<wgpu::Buffer>,
    bind_groups: Vec<Rc<wgpu::BindGroup>>,
    cursor: usize,
}

impl Default for ModelUniformPool {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelUniformPool {
    pub fn new() -> Self {
        Self {
            buffers: Vec::new(),
            bind_groups: Vec::new(),
            cursor: 0,
        }
    }

    /// Marks the start of a frame: the slots are reused from the top.
    pub fn reset(&mut self) {
        self.cursor = 0;
    }

    /// Number of slots the pool currently owns (one per draw call of the
    /// busiest frame so far).
    pub fn len(&self) -> usize {
        self.buffers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buffers.is_empty()
    }

    /// Returns the bind group for the next slot and writes the uniform data
    /// into the slot's buffer.
    ///
    /// The bind group is handed out behind an `Rc` (a cheap handle clone) so
    /// the caller can collect the groups for the render pass without
    /// borrowing the pool. `wgpu::BindGroup` is not `Clone` in wgpu 0.19.
    pub fn acquire(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        model: glam::Mat4,
        color: [f32; 4],
    ) -> Rc<wgpu::BindGroup> {
        let uniform = ModelUniform {
            model: model.to_cols_array_2d(),
            color,
        };

        if self.cursor >= self.buffers.len() {
            let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Pooled Model Uniform Buffer"),
                contents: bytemuck::cast_slice(&[uniform]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Pooled Model Bind Group"),
                layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                }],
            });

            self.buffers.push(buffer);
            self.bind_groups.push(Rc::new(bind_group));
        } else {
            // Reuse the slot: only the contents change.
            queue.write_buffer(
                &self.buffers[self.cursor],
                0,
                bytemuck::cast_slice(&[uniform]),
            );
        }

        let bind_group = Rc::clone(&self.bind_groups[self.cursor]);
        self.cursor += 1;
        bind_group
    }
}

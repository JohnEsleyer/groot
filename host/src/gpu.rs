use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use std::sync::Arc;
use wgpu::util::DeviceExt;
use wgpu::*;

/// Number of `f32` values per Groot sprite vertex:
/// `[x, y, u, v, r, g, b, a]`.
pub const FLOATS_PER_VERTEX: usize = 8;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ViewportUniform {
    // x, y = framebuffer size in pixels; zw reserved for future camera data.
    size: [f32; 4],
}

pub struct GpuRenderer {
    pub instance: Instance,
    surface: Option<Surface<'static>>,
    device: Option<Device>,
    queue: Option<Queue>,
    config: Option<SurfaceConfiguration>,
    pipeline: Option<RenderPipeline>,
    vertex_buffer: Option<Buffer>,
    vertex_buffer_capacity: usize,
    vertex_count: u32,
    viewport_buffer: Option<Buffer>,
    bind_group: Option<BindGroup>,
    pub clear_color: [f64; 4],
}

impl GpuRenderer {
    pub fn new() -> Self {
        let instance = Instance::new(InstanceDescriptor {
            backends: Backends::PRIMARY,
            ..Default::default()
        });
        Self {
            instance,
            surface: None,
            device: None,
            queue: None,
            config: None,
            pipeline: None,
            vertex_buffer: None,
            vertex_buffer_capacity: 0,
            vertex_count: 0,
            viewport_buffer: None,
            bind_group: None,
            clear_color: [0.06, 0.06, 0.09, 1.0],
        }
    }

    pub fn init_surface<W>(&mut self, window: Arc<W>, width: u32, height: u32)
    where
        W: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static,
    {
        let surface = self
            .instance
            .create_surface(window)
            .expect("Failed to create wgpu surface");

        let adapter = pollster::block_on(self.instance.request_adapter(&RequestAdapterOptions {
            power_preference: PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .expect("No suitable GPU adapter found");

        let (device, queue) = pollster::block_on(adapter.request_device(
            &DeviceDescriptor {
                label: Some("Groot GPU Device"),
                required_features: Features::empty(),
                required_limits: Limits::default(),
            },
            None,
        ))
        .expect("Failed to create GPU device");

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);

        let width = width.max(1);
        let height = height.max(1);
        let config = SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format,
            width,
            height,
            present_mode: PresentMode::AutoVsync,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        self.surface = Some(surface);
        self.device = Some(device);
        self.queue = Some(queue);
        self.config = Some(config);

        self.create_pipeline();
        self.update_viewport();
    }

    fn create_pipeline(&mut self) {
        let device = self.device.as_ref().unwrap();
        let config = self.config.as_ref().unwrap();

        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("Groot Sprite Shader"),
            source: ShaderSource::Wgsl(
                r#"
                struct Viewport {
                    size: vec4<f32>,
                };

                @group(0) @binding(0)
                var<uniform> viewport: Viewport;

                struct VertexInput {
                    @location(0) position: vec2<f32>,
                    @location(1) uv: vec2<f32>,
                    @location(2) color: vec4<f32>,
                };

                struct VertexOutput {
                    @builtin(position) clip_position: vec4<f32>,
                    @location(0) uv: vec2<f32>,
                    @location(1) color: vec4<f32>,
                };

                @vertex
                fn vs_main(model: VertexInput) -> VertexOutput {
                    var out: VertexOutput;
                    // The engine works in pixels with a top-left origin.
                    let w = max(viewport.size.x, 1.0);
                    let h = max(viewport.size.y, 1.0);
                    let ndc = vec2<f32>(
                        (model.position.x / w) * 2.0 - 1.0,
                        1.0 - (model.position.y / h) * 2.0
                    );
                    out.clip_position = vec4<f32>(ndc, 0.0, 1.0);
                    out.uv = model.uv;
                    out.color = model.color;
                    return out;
                }

                @fragment
                fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
                    return in.color;
                }
            "#
                .into(),
            ),
        });

        let viewport_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("Viewport Bind Group Layout"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("Groot Pipeline Layout"),
            bind_group_layouts: &[&viewport_layout],
            push_constant_ranges: &[],
        });

        let stride = (FLOATS_PER_VERTEX * std::mem::size_of::<f32>()) as BufferAddress;
        let vertex_layout = VertexBufferLayout {
            array_stride: stride,
            step_mode: VertexStepMode::Vertex,
            attributes: &[
                VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: VertexFormat::Float32x2,
                },
                VertexAttribute {
                    offset: (2 * std::mem::size_of::<f32>()) as BufferAddress,
                    shader_location: 1,
                    format: VertexFormat::Float32x2,
                },
                VertexAttribute {
                    offset: (4 * std::mem::size_of::<f32>()) as BufferAddress,
                    shader_location: 2,
                    format: VertexFormat::Float32x4,
                },
            ],
        };

        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("Groot Sprite Pipeline"),
            layout: Some(&layout),
            vertex: VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[vertex_layout],
            },
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(ColorTargetState {
                    format: config.format,
                    blend: Some(BlendState::ALPHA_BLENDING),
                    write_mask: ColorWrites::ALL,
                })],
            }),
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: MultisampleState::default(),
            multiview: None,
        });

        let viewport_buffer = device.create_buffer_init(&util::BufferInitDescriptor {
            label: Some("Viewport Uniform"),
            contents: bytemuck::bytes_of(&ViewportUniform { size: [1.0, 1.0, 0.0, 0.0] }),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });

        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("Viewport Bind Group"),
            layout: &viewport_layout,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: viewport_buffer.as_entire_binding(),
            }],
        });

        self.pipeline = Some(pipeline);
        self.viewport_buffer = Some(viewport_buffer);
        self.bind_group = Some(bind_group);
    }

    fn update_viewport(&self) {
        let (Some(queue), Some(buffer), Some(config)) =
            (self.queue.as_ref(), self.viewport_buffer.as_ref(), self.config.as_ref())
        else {
            return;
        };
        let uniform = ViewportUniform {
            size: [config.width as f32, config.height as f32, 0.0, 0.0],
        };
        queue.write_buffer(buffer, 0, bytemuck::bytes_of(&uniform));
    }

    /// Upload a flat vertex buffer. Six vertices per un-indexed quad.
    pub fn upload_vertices(&mut self, floats: &[f32]) {
        let (Some(device), Some(queue)) = (self.device.as_ref(), self.queue.as_ref()) else {
            return;
        };

        self.vertex_count = (floats.len() / FLOATS_PER_VERTEX) as u32;
        if floats.is_empty() {
            return;
        }

        let byte_len = floats.len() * std::mem::size_of::<f32>();
        if self.vertex_buffer.is_none() || byte_len > self.vertex_buffer_capacity {
            let new_cap = byte_len.max(1024).next_power_of_two();
            let buffer = device.create_buffer(&BufferDescriptor {
                label: Some("Groot Dynamic Vertex Buffer"),
                size: new_cap as u64,
                usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.vertex_buffer = Some(buffer);
            self.vertex_buffer_capacity = new_cap;
        }

        if let Some(buf) = self.vertex_buffer.as_ref() {
            queue.write_buffer(buf, 0, bytemuck::cast_slice(floats));
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        let (Some(surface), Some(device), Some(config)) = (
            self.surface.as_ref(),
            self.device.as_ref(),
            self.config.as_mut(),
        ) else {
            return;
        };
        config.width = width;
        config.height = height;
        surface.configure(device, config);
        self.update_viewport();
    }

    pub fn draw_batch(&mut self, vertex_count: u32, r: f64, g: f64, b: f64, a: f64) {
        let (Some(surface), Some(device), Some(queue)) = (
            self.surface.as_ref(),
            self.device.as_ref(),
            self.queue.as_ref(),
        ) else {
            return;
        };
        let (Some(pipeline), Some(bind_group)) = (self.pipeline.as_ref(), self.bind_group.as_ref())
        else {
            return;
        };

        let output = match surface.get_current_texture() {
            Ok(tex) => tex,
            Err(_) => return,
        };
        let view = output.texture.create_view(&TextureViewDescriptor::default());

        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("Groot Frame Encoder"),
        });

        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("Groot Render Pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(Color { r, g, b, a }),
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            if vertex_count > 0 {
                if let Some(vb) = self.vertex_buffer.as_ref() {
                    pass.set_pipeline(pipeline);
                    pass.set_bind_group(0, bind_group, &[]);
                    pass.set_vertex_buffer(0, vb.slice(..));
                    pass.draw(0..vertex_count, 0..1);
                }
            }
        }

        queue.submit(std::iter::once(encoder.finish()));
        output.present();
    }

    pub fn draw_frame(&mut self, r: f64, g: f64, b: f64) {
        self.clear_color = [r, g, b, 1.0];
        let count = self.vertex_count;
        self.draw_batch(count, r, g, b, 1.0);
    }

    pub fn redraw(&mut self) {
        let c = self.clear_color;
        let count = self.vertex_count;
        self.draw_batch(count, c[0], c[1], c[2], c[3]);
    }
}

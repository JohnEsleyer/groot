use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use std::collections::HashMap;
use std::sync::Arc;
use wgpu::util::DeviceExt;
use wgpu::*;

/// Number of `f32` values per Groot sprite vertex:
/// `[x, y, u, v, r, g, b, a]`.
pub const FLOATS_PER_VERTEX: usize = 8;

/// Floats per 3D mesh vertex:
/// `[x, y, z, nx, ny, nz, u, v]` (8 floats = 32 bytes)
pub const FLOATS_PER_VERTEX3D: usize = 8;

/// Floats per 3D instance:
/// `Mat4` (16 floats) + optional tint (4 floats)
/// If 16 floats: model matrix only, default tint (1,1,1,1).
/// If 20 floats: model matrix + tint rgba.
pub const FLOATS_PER_INSTANCE3D: usize = 20;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ViewportUniform {
    size: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Camera3DUniform {
    view_proj: [f32; 16],
    camera_pos: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Light3DUniform {
    direction: [f32; 4],
    color: [f32; 4],
}

pub struct MeshData {
    pub vertex_buffer: Buffer,
    pub index_buffer: Buffer,
    pub index_count: u32,
}

pub struct InstanceBatch {
    pub mesh_id: u32,
    pub instance_count: u32,
    pub instance_buffer: Buffer,
}

pub struct GpuRenderer {
    pub instance: Instance,
    surface: Option<Surface<'static>>,
    device: Option<Device>,
    queue: Option<Queue>,
    config: Option<SurfaceConfiguration>,

    // 2D Pipeline
    pipeline: Option<RenderPipeline>,
    vertex_buffer: Option<Buffer>,
    vertex_buffer_capacity: usize,
    vertex_count: u32,
    viewport_buffer: Option<Buffer>,
    bind_group: Option<BindGroup>,

    // 3D Pipeline
    depth_texture: Option<Texture>,
    depth_view: Option<TextureView>,
    pipeline_3d: Option<RenderPipeline>,
    camera_buffer_3d: Option<Buffer>,
    light_buffer_3d: Option<Buffer>,
    bind_group_3d: Option<BindGroup>,
    meshes: HashMap<u32, MeshData>,
    next_mesh_id: u32,
    pending_draws_3d: Vec<InstanceBatch>,

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
            depth_texture: None,
            depth_view: None,
            pipeline_3d: None,
            camera_buffer_3d: None,
            light_buffer_3d: None,
            bind_group_3d: None,
            meshes: HashMap::new(),
            next_mesh_id: 1,
            pending_draws_3d: Vec::new(),
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

        self.create_depth_texture(width, height);
        self.create_pipeline();
        self.create_pipeline_3d();
        self.update_viewport();
    }

    fn create_depth_texture(&mut self, width: u32, height: u32) {
        let Some(device) = self.device.as_ref() else {
            return;
        };
        let depth_desc = TextureDescriptor {
            label: Some("Groot Depth Texture"),
            size: Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Depth32Float,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        };
        let depth_texture = device.create_texture(&depth_desc);
        let depth_view = depth_texture.create_view(&TextureViewDescriptor::default());
        self.depth_texture = Some(depth_texture);
        self.depth_view = Some(depth_view);
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

    fn create_pipeline_3d(&mut self) {
        let device = self.device.as_ref().unwrap();
        let config = self.config.as_ref().unwrap();

        let shader_3d = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("Groot 3D Forward Shader"),
            source: ShaderSource::Wgsl(
                r#"
                struct CameraUniform {
                    view_proj: mat4x4<f32>,
                    camera_pos: vec4<f32>,
                };

                struct LightUniform {
                    direction: vec4<f32>,
                    color: vec4<f32>,
                };

                @group(0) @binding(0) var<uniform> camera: CameraUniform;
                @group(0) @binding(1) var<uniform> light: LightUniform;

                struct VertexInput {
                    @location(0) position: vec3<f32>,
                    @location(1) normal: vec3<f32>,
                    @location(2) uv: vec2<f32>,
                };

                struct InstanceInput {
                    @location(3) model_matrix_0: vec4<f32>,
                    @location(4) model_matrix_1: vec4<f32>,
                    @location(5) model_matrix_2: vec4<f32>,
                    @location(6) model_matrix_3: vec4<f32>,
                    @location(7) tint_color: vec4<f32>,
                };

                struct VertexOutput {
                    @builtin(position) clip_position: vec4<f32>,
                    @location(0) world_normal: vec3<f32>,
                    @location(1) uv: vec2<f32>,
                    @location(2) tint: vec4<f32>,
                };

                @vertex
                fn vs_main(vertex: VertexInput, instance: InstanceInput) -> VertexOutput {
                    let model = mat4x4<f32>(
                        instance.model_matrix_0,
                        instance.model_matrix_1,
                        instance.model_matrix_2,
                        instance.model_matrix_3,
                    );
                    var out: VertexOutput;
                    let world_pos = model * vec4<f32>(vertex.position, 1.0);
                    out.clip_position = camera.view_proj * world_pos;
                    out.world_normal = normalize((model * vec4<f32>(vertex.normal, 0.0)).xyz);
                    out.uv = vertex.uv;
                    out.tint = instance.tint_color;
                    return out;
                }

                @fragment
                fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
                    let light_dir = normalize(-light.direction.xyz);
                    let diff = max(dot(in.world_normal, light_dir), 0.0);
                    let ambient = 0.15;
                    let final_light = ambient + diff * 0.85;
                    return vec4<f32>(in.tint.rgb * final_light, in.tint.a);
                }
            "#
                .into(),
            ),
        });

        let bind_group_layout_3d = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("Groot 3D Bind Group Layout"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let layout_3d = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("Groot 3D Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout_3d],
            push_constant_ranges: &[],
        });

        let vertex_layout_3d = VertexBufferLayout {
            array_stride: (FLOATS_PER_VERTEX3D * std::mem::size_of::<f32>()) as BufferAddress,
            step_mode: VertexStepMode::Vertex,
            attributes: &[
                VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: VertexFormat::Float32x3,
                },
                VertexAttribute {
                    offset: (3 * std::mem::size_of::<f32>()) as BufferAddress,
                    shader_location: 1,
                    format: VertexFormat::Float32x3,
                },
                VertexAttribute {
                    offset: (6 * std::mem::size_of::<f32>()) as BufferAddress,
                    shader_location: 2,
                    format: VertexFormat::Float32x2,
                },
            ],
        };

        let instance_layout_3d = VertexBufferLayout {
            array_stride: (FLOATS_PER_INSTANCE3D * std::mem::size_of::<f32>()) as BufferAddress,
            step_mode: VertexStepMode::Instance,
            attributes: &[
                VertexAttribute {
                    offset: 0,
                    shader_location: 3,
                    format: VertexFormat::Float32x4,
                },
                VertexAttribute {
                    offset: (4 * std::mem::size_of::<f32>()) as BufferAddress,
                    shader_location: 4,
                    format: VertexFormat::Float32x4,
                },
                VertexAttribute {
                    offset: (8 * std::mem::size_of::<f32>()) as BufferAddress,
                    shader_location: 5,
                    format: VertexFormat::Float32x4,
                },
                VertexAttribute {
                    offset: (12 * std::mem::size_of::<f32>()) as BufferAddress,
                    shader_location: 6,
                    format: VertexFormat::Float32x4,
                },
                VertexAttribute {
                    offset: (16 * std::mem::size_of::<f32>()) as BufferAddress,
                    shader_location: 7,
                    format: VertexFormat::Float32x4,
                },
            ],
        };

        let pipeline_3d = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("Groot 3D Render Pipeline"),
            layout: Some(&layout_3d),
            vertex: VertexState {
                module: &shader_3d,
                entry_point: "vs_main",
                buffers: &[vertex_layout_3d, instance_layout_3d],
            },
            fragment: Some(FragmentState {
                module: &shader_3d,
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
                cull_mode: Some(Face::Back),
                polygon_mode: PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(DepthStencilState {
                format: TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: CompareFunction::Less,
                stencil: StencilState::default(),
                bias: DepthBiasState::default(),
            }),
            multisample: MultisampleState::default(),
            multiview: None,
        });

        let default_cam = Camera3DUniform {
            view_proj: [
                1.0, 0.0, 0.0, 0.0,
                0.0, 1.0, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                0.0, 0.0, 0.0, 1.0,
            ],
            camera_pos: [0.0, 0.0, 0.0, 1.0],
        };
        let camera_buffer_3d = device.create_buffer_init(&util::BufferInitDescriptor {
            label: Some("Camera 3D Uniform"),
            contents: bytemuck::bytes_of(&default_cam),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });

        let default_light = Light3DUniform {
            direction: [-0.5, -1.0, -0.3, 0.0],
            color: [1.0, 1.0, 1.0, 1.0],
        };
        let light_buffer_3d = device.create_buffer_init(&util::BufferInitDescriptor {
            label: Some("Light 3D Uniform"),
            contents: bytemuck::bytes_of(&default_light),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });

        let bind_group_3d = device.create_bind_group(&BindGroupDescriptor {
            label: Some("Groot 3D Bind Group"),
            layout: &bind_group_layout_3d,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer_3d.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: light_buffer_3d.as_entire_binding(),
                },
            ],
        });

        self.pipeline_3d = Some(pipeline_3d);
        self.camera_buffer_3d = Some(camera_buffer_3d);
        self.light_buffer_3d = Some(light_buffer_3d);
        self.bind_group_3d = Some(bind_group_3d);
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
        self.create_depth_texture(width, height);
        self.update_viewport();
    }

    // --- 2D Methods ---

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

    // --- 3D Mesh & Instance Management ---

    pub fn upload_mesh(&mut self, vertices: &[f32], indices: &[u32]) -> u32 {
        let Some(device) = self.device.as_ref() else {
            return 0;
        };

        let vertex_buffer = device.create_buffer_init(&util::BufferInitDescriptor {
            label: Some("Groot 3D Mesh Vertex Buffer"),
            contents: bytemuck::cast_slice(vertices),
            usage: BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&util::BufferInitDescriptor {
            label: Some("Groot 3D Mesh Index Buffer"),
            contents: bytemuck::cast_slice(indices),
            usage: BufferUsages::INDEX,
        });

        let id = self.next_mesh_id;
        self.next_mesh_id += 1;

        self.meshes.insert(
            id,
            MeshData {
                vertex_buffer,
                index_buffer,
                index_count: indices.len() as u32,
            },
        );

        id
    }

    pub fn destroy_mesh(&mut self, id: u32) {
        self.meshes.remove(&id);
    }

    pub fn set_camera_3d(&mut self, view_proj: &[f32], eye_pos: &[f32]) {
        let (Some(queue), Some(buf)) = (self.queue.as_ref(), self.camera_buffer_3d.as_ref()) else {
            return;
        };
        let mut uniform = Camera3DUniform {
            view_proj: [0.0; 16],
            camera_pos: [0.0; 4],
        };
        let vp_len = view_proj.len().min(16);
        uniform.view_proj[..vp_len].copy_from_slice(&view_proj[..vp_len]);
        let eye_len = eye_pos.len().min(3);
        uniform.camera_pos[..eye_len].copy_from_slice(&eye_pos[..eye_len]);
        uniform.camera_pos[3] = 1.0;
        queue.write_buffer(buf, 0, bytemuck::bytes_of(&uniform));
    }

    pub fn set_light_3d(&mut self, dir: &[f32], color: &[f32], intensity: f32) {
        let (Some(queue), Some(buf)) = (self.queue.as_ref(), self.light_buffer_3d.as_ref()) else {
            return;
        };
        let mut uniform = Light3DUniform {
            direction: [0.0, -1.0, 0.0, 0.0],
            color: [1.0, 1.0, 1.0, 1.0],
        };
        let dir_len = dir.len().min(3);
        uniform.direction[..dir_len].copy_from_slice(&dir[..dir_len]);
        let col_len = color.len().min(3);
        for i in 0..col_len {
            uniform.color[i] = color[i] * intensity;
        }
        queue.write_buffer(buf, 0, bytemuck::bytes_of(&uniform));
    }

    pub fn draw_instances_3d(&mut self, mesh_id: u32, instance_data: &[f32]) {
        let Some(device) = self.device.as_ref() else {
            return;
        };
        if instance_data.is_empty() || !self.meshes.contains_key(&mesh_id) {
            return;
        }

        // Each instance can be 16 floats (Mat4) or 20 floats (Mat4 + tint)
        let (instance_count, packed_data) = if instance_data.len() % 20 == 0 {
            (
                (instance_data.len() / 20) as u32,
                instance_data.to_vec(),
            )
        } else if instance_data.len() % 16 == 0 {
            let count = instance_data.len() / 16;
            let mut packed = Vec::with_capacity(count * 20);
            for i in 0..count {
                packed.extend_from_slice(&instance_data[i * 16..(i + 1) * 16]);
                packed.extend_from_slice(&[1.0, 1.0, 1.0, 1.0]); // default tint
            }
            (count as u32, packed)
        } else {
            return;
        };

        let instance_buffer = device.create_buffer_init(&util::BufferInitDescriptor {
            label: Some("Groot 3D Instance Buffer"),
            contents: bytemuck::cast_slice(&packed_data),
            usage: BufferUsages::VERTEX,
        });

        self.pending_draws_3d.push(InstanceBatch {
            mesh_id,
            instance_count,
            instance_buffer,
        });
    }

    pub fn draw_batch_all(
        &mut self,
        vertex_count_2d: u32,
        r: f64,
        g: f64,
        b: f64,
        a: f64,
    ) {
        let (Some(surface), Some(device), Some(queue)) = (
            self.surface.as_ref(),
            self.device.as_ref(),
            self.queue.as_ref(),
        ) else {
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
            let depth_attachment = self.depth_view.as_ref().map(|dview| {
                RenderPassDepthStencilAttachment {
                    view: dview,
                    depth_ops: Some(Operations {
                        load: LoadOp::Clear(1.0),
                        store: StoreOp::Store,
                    }),
                    stencil_ops: None,
                }
            });

            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("Groot Combined Render Pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(Color { r, g, b, a }),
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: depth_attachment,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // 1. Render 3D instances first (with depth testing & writing)
            if !self.pending_draws_3d.is_empty() {
                if let (Some(pipeline_3d), Some(bind_group_3d)) =
                    (self.pipeline_3d.as_ref(), self.bind_group_3d.as_ref())
                {
                    pass.set_pipeline(pipeline_3d);
                    pass.set_bind_group(0, bind_group_3d, &[]);

                    for batch in &self.pending_draws_3d {
                        if let Some(mesh) = self.meshes.get(&batch.mesh_id) {
                            pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                            pass.set_vertex_buffer(1, batch.instance_buffer.slice(..));
                            pass.set_index_buffer(mesh.index_buffer.slice(..), IndexFormat::Uint32);
                            pass.draw_indexed(0..mesh.index_count, 0, 0..batch.instance_count);
                        }
                    }
                }
            }

            // 2. Render 2D sprites on top
            if vertex_count_2d > 0 {
                if let (Some(pipeline), Some(bind_group), Some(vb)) = (
                    self.pipeline.as_ref(),
                    self.bind_group.as_ref(),
                    self.vertex_buffer.as_ref(),
                ) {
                    pass.set_pipeline(pipeline);
                    pass.set_bind_group(0, bind_group, &[]);
                    pass.set_vertex_buffer(0, vb.slice(..));
                    pass.draw(0..vertex_count_2d, 0..1);
                }
            }
        }

        queue.submit(std::iter::once(encoder.finish()));
        self.pending_draws_3d.clear();
        output.present();
    }

    pub fn draw_frame(&mut self, r: f64, g: f64, b: f64) {
        self.clear_color = [r, g, b, 1.0];
        let count = self.vertex_count;
        self.draw_batch_all(count, r, g, b, 1.0);
    }

    pub fn redraw(&mut self) {
        let c = self.clear_color;
        let count = self.vertex_count;
        self.draw_batch_all(count, c[0], c[1], c[2], c[3]);
    }
}

use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex3D {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

impl Vertex3D {
    pub fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex3D>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: (std::mem::size_of::<[f32; 3]>() * 2) as wgpu::BufferAddress,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x2,
                },
            ],
        }
    }
}

pub struct Mesh {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub num_indices: u32,
}

impl Mesh {
    pub fn cuboid(device: &wgpu::Device, x: f32, y: f32, z: f32) -> Self {
        let hx = x * 0.5;
        let hy = y * 0.5;
        let hz = z * 0.5;

        #[rustfmt::skip]
        let vertices = [
            Vertex3D { position: [-hx, -hy,  hz], normal: [ 0.0,  0.0,  1.0], uv: [0.0, 1.0] },
            Vertex3D { position: [ hx, -hy,  hz], normal: [ 0.0,  0.0,  1.0], uv: [1.0, 1.0] },
            Vertex3D { position: [ hx,  hy,  hz], normal: [ 0.0,  0.0,  1.0], uv: [1.0, 0.0] },
            Vertex3D { position: [-hx,  hy,  hz], normal: [ 0.0,  0.0,  1.0], uv: [0.0, 0.0] },
            Vertex3D { position: [ hx, -hy, -hz], normal: [ 0.0,  0.0, -1.0], uv: [0.0, 1.0] },
            Vertex3D { position: [-hx, -hy, -hz], normal: [ 0.0,  0.0, -1.0], uv: [1.0, 1.0] },
            Vertex3D { position: [-hx,  hy, -hz], normal: [ 0.0,  0.0, -1.0], uv: [1.0, 0.0] },
            Vertex3D { position: [ hx,  hy, -hz], normal: [ 0.0,  0.0, -1.0], uv: [0.0, 0.0] },
            Vertex3D { position: [-hx,  hy,  hz], normal: [ 0.0,  1.0,  0.0], uv: [0.0, 1.0] },
            Vertex3D { position: [ hx,  hy,  hz], normal: [ 0.0,  1.0,  0.0], uv: [1.0, 1.0] },
            Vertex3D { position: [ hx,  hy, -hz], normal: [ 0.0,  1.0,  0.0], uv: [1.0, 0.0] },
            Vertex3D { position: [-hx,  hy, -hz], normal: [ 0.0,  1.0,  0.0], uv: [0.0, 0.0] },
            Vertex3D { position: [-hx, -hy, -hz], normal: [ 0.0, -1.0,  0.0], uv: [0.0, 1.0] },
            Vertex3D { position: [ hx, -hy, -hz], normal: [ 0.0, -1.0,  0.0], uv: [1.0, 1.0] },
            Vertex3D { position: [ hx, -hy,  hz], normal: [ 0.0, -1.0,  0.0], uv: [1.0, 0.0] },
            Vertex3D { position: [-hx, -hy,  hz], normal: [ 0.0, -1.0,  0.0], uv: [0.0, 0.0] },
            Vertex3D { position: [ hx, -hy,  hz], normal: [ 1.0,  0.0,  0.0], uv: [0.0, 1.0] },
            Vertex3D { position: [ hx, -hy, -hz], normal: [ 1.0,  0.0,  0.0], uv: [1.0, 1.0] },
            Vertex3D { position: [ hx,  hy, -hz], normal: [ 1.0,  0.0,  0.0], uv: [1.0, 0.0] },
            Vertex3D { position: [ hx,  hy,  hz], normal: [ 1.0,  0.0,  0.0], uv: [0.0, 0.0] },
            Vertex3D { position: [-hx, -hy, -hz], normal: [-1.0,  0.0,  0.0], uv: [0.0, 1.0] },
            Vertex3D { position: [-hx, -hy,  hz], normal: [-1.0,  0.0,  0.0], uv: [1.0, 1.0] },
            Vertex3D { position: [-hx,  hy,  hz], normal: [-1.0,  0.0,  0.0], uv: [1.0, 0.0] },
            Vertex3D { position: [-hx,  hy, -hz], normal: [-1.0,  0.0,  0.0], uv: [0.0, 0.0] },
        ];

        #[rustfmt::skip]
        let indices: [u16; 36] = [
            0, 1, 2,  0, 2, 3,
            4, 5, 6,  4, 6, 7,
            8, 9, 10, 8, 10, 11,
            12, 13, 14, 12, 14, 15,
            16, 17, 18, 16, 18, 19,
            20, 21, 22, 20, 22, 23,
        ];

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Cuboid Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Cuboid Index Buffer"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        Self {
            vertex_buffer,
            index_buffer,
            num_indices: indices.len() as u32,
        }
    }
}

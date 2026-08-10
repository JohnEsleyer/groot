mod assets;
mod ecs;
mod groot_module;
mod render;

use std::sync::Arc;
use assets::*;
use ecs::*;
use render::*;
use wgpu::util::DeviceExt;
use winit::{
    event::*,
    event_loop::EventLoop,
    window::WindowBuilder,
};

fn main() {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    let config = GrootConfig::load("assets/config.ron");
    log::info!("Starting Groot Engine: {}", config.project.name);

    let event_loop = EventLoop::new().expect("Failed to create EventLoop");
    let window = Arc::new(
        WindowBuilder::new()
            .with_title(&config.window.title)
            .with_inner_size(winit::dpi::LogicalSize::new(
                config.window.width,
                config.window.height,
            ))
            .build(&event_loop)
            .expect("Failed to create Window"),
    );

    let mut render_ctx = pollster::block_on(RenderContext::new(
        Arc::clone(&window),
        config.render.clear_color,
    ));

    let pipeline_3d = Pipeline3D::new(&render_ctx.device, render_ctx.config.format);
    let mut camera = Camera3D::new(render_ctx.config.width as f32, render_ctx.config.height as f32);

    let mut camera_uniform = camera.build_uniform();
    let camera_buffer = render_ctx.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Camera Buffer"),
        contents: bytemuck::cast_slice(&[camera_uniform]),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    let camera_bind_group = render_ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Camera Bind Group"),
        layout: &pipeline_3d.camera_bind_group_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: camera_buffer.as_entire_binding(),
        }],
    });

    let cube_mesh = Mesh::cuboid(&render_ctx.device, 1.0, 1.0, 1.0);
    let mut world = World::new();

    spawn_scene(&mut world, &config.initial_scene);

    let _ = event_loop.run(move |event, target| match event {
        Event::WindowEvent { ref event, window_id } if window_id == window.id() => match event {
            WindowEvent::CloseRequested => target.exit(),
            WindowEvent::Resized(physical_size) => {
                render_ctx.resize(*physical_size);
                camera.aspect = physical_size.width as f32 / physical_size.height.max(1) as f32;
            }
            WindowEvent::RedrawRequested => {
                camera_uniform = camera.build_uniform();
                render_ctx.queue.write_buffer(&camera_buffer, 0, bytemuck::cast_slice(&[camera_uniform]));

                let output = match render_ctx.surface.get_current_texture() {
                    Ok(tex) => tex,
                    Err(wgpu::SurfaceError::Lost) => {
                        render_ctx.resize(render_ctx.size);
                        return;
                    }
                    Err(wgpu::SurfaceError::OutOfMemory) => {
                        target.exit();
                        return;
                    }
                    Err(e) => {
                        log::error!("Surface error: {e:?}");
                        return;
                    }
                };

                let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
                let mut encoder = render_ctx.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Main Encoder"),
                });

                let model_bind_groups: Vec<(wgpu::BindGroup, wgpu::Buffer)> = world
                    .query_mut::<(&Transform3D, &Visual3D)>()
                    .into_iter()
                    .map(|(_id, (tf, vis))| {
                        pipeline_3d.create_model_bind_group(&render_ctx.device, tf.matrix(), vis.color)
                    })
                    .collect();

                {
                    let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("3D Scene Pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(render_ctx.clear_color),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                            view: &render_ctx.depth_texture_view,
                            depth_ops: Some(wgpu::Operations {
                                load: wgpu::LoadOp::Clear(1.0),
                                store: wgpu::StoreOp::Store,
                            }),
                            stencil_ops: None,
                        }),
                        occlusion_query_set: None,
                        timestamp_writes: None,
                    });

                    render_pass.set_pipeline(&pipeline_3d.render_pipeline);
                    render_pass.set_bind_group(0, &camera_bind_group, &[]);

                    for (bind_group, _buf) in &model_bind_groups {
                        render_pass.set_bind_group(1, bind_group, &[]);
                        render_pass.set_vertex_buffer(0, cube_mesh.vertex_buffer.slice(..));
                        render_pass.set_index_buffer(cube_mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                        render_pass.draw_indexed(0..cube_mesh.num_indices, 0, 0..1);
                    }
                }

                render_ctx.queue.submit(std::iter::once(encoder.finish()));
                output.present();
            }
            _ => {}
        },
        Event::AboutToWait => {
            window.request_redraw();
        }
        _ => {}
    });
}
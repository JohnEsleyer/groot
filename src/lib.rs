pub mod assets;
pub mod ecs;
pub mod platform;
pub mod render;
pub mod script;
pub mod groot_module;
pub mod plugin;

pub use assets::GrootConfig;
pub use assets::spawn_scene;
pub use ecs::*;
pub use render::*;
pub use script::*;

use std::sync::Arc;
use std::time::Instant;

use wgpu::util::DeviceExt;
use winit::{
    event::*,
    event_loop::EventLoop,
    window::WindowBuilder,
};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

/// Synchronous entry point for native desktop platforms (Linux / Windows / macOS).
pub fn run_game(config_path: &str) {
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    {
        platform::init_platform_logging();
        let event_loop = platform::desktop::create_event_loop();
        pollster::block_on(run_game_with_event_loop(event_loop, config_path));
    }
    #[cfg(any(target_arch = "wasm32", target_os = "android"))]
    {
        log::warn!(
            "run_game is not the entry point on this target; \
             use run_wasm (web) or android_main (Android)."
        );
        let _ = config_path;
    }
}

/// WASM entry point exported to the web browser.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn run_wasm() {
    platform::init_platform_logging();

    wasm_bindgen_futures::spawn_local(async {
        let event_loop = EventLoop::new().expect("Failed to create Web EventLoop");
        run_game_with_event_loop(event_loop, "assets/config.ron").await;
    });
}

/// Non-blocking async runner compatible with every platform. The caller
/// supplies the event loop so Android can attach its `AndroidApp` first.
pub async fn run_game_with_event_loop(event_loop: EventLoop<()>, config_path: &str) {
    let config = GrootConfig::load(config_path);
    log::info!("Starting Groot Engine: {}", config.project.name);

    // Attach the winit window to the HTML canvas on WASM builds.
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
    platform::web::mount_canvas(&window);

    let mut render_ctx = RenderContext::new(
        Arc::clone(&window),
        config.render.clear_color,
    ).await;

    let pipeline_3d = Pipeline3D::new(&render_ctx.device, render_ctx.config.format);
    let mut camera = Camera3D::new(render_ctx.config.width as f32, render_ctx.config.height as f32);

    let initial_camera_uniform = camera.build_uniform();
    let camera_buffer = render_ctx.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Camera Buffer"),
        contents: bytemuck::cast_slice(&[initial_camera_uniform]),
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
    let mut script_host = GrootScriptHost::new();

    spawn_scene(&mut world, &config.initial_scene);

    let mut last_frame_time = Instant::now();

    let _ = event_loop.run(move |event, target| match event {
        // Mobile lifecycle: surfaces are invalidated when the app is paused or
        // rotated. Reconfigure the surface when the activity is resumed.
        Event::Resumed => {
            if render_ctx.size.width > 0 && render_ctx.size.height > 0 {
                render_ctx.resize(render_ctx.size);
            }
        }
        Event::WindowEvent { ref event, window_id } if window_id == window.id() => {
            script_host.input.process_event(event);

            match event {
                WindowEvent::CloseRequested => target.exit(),
                WindowEvent::Resized(physical_size) => {
                    render_ctx.resize(*physical_size);
                    camera.aspect = physical_size.width as f32 / physical_size.height.max(1) as f32;
                }
                WindowEvent::RedrawRequested => {
                    let now = Instant::now();
                    let dt = (now - last_frame_time).as_secs_f64();
                    last_frame_time = now;

                    update_scripts(&mut script_host, &mut world, dt);
                    script_host.input.reset_frame_input();

                    let camera_uniform = camera.build_uniform();
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
                            label: Some("3D Render Pass"),
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
            }
        }
        Event::AboutToWait => {
            window.request_redraw();
        }
        _ => {}
    });
}

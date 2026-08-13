pub mod assets;
pub mod ecs;
pub mod groot_module;
pub mod platform;
pub mod plugin;
pub mod render;
pub mod script;

pub use assets::spawn_scene;
pub use assets::GrootConfig;
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

/// Synchronous entry point for native desktop platforms (Linux / Windows / macOS).
pub fn run_game(config_path: &str) {
    #[cfg(not(target_os = "android"))]
    {
        platform::init_platform_logging();
        let event_loop = platform::desktop::create_event_loop();
        pollster::block_on(run_game_with_event_loop(event_loop, config_path));
    }
    #[cfg(target_os = "android")]
    {
        log::warn!(
            "run_game is not the entry point on Android; use android_main."
        );
        let _ = config_path;
    }
}

/// GPU state that must live for the whole run. Created lazily on the first
/// `Event::Resumed` because the native window surface does not exist before then
/// (on Android in particular), and must be re-created on suspend/resume.
struct RenderState<'a> {
    render_ctx: RenderContext<'a>,
    pipeline_3d: Pipeline3D,
    camera: Camera3D,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    cube_mesh: Mesh,
    world: World,
    script_host: GrootScriptHost,
}

// ---------------------------------------------------------------------------
// Event Loop Runner (wgpu)
// ---------------------------------------------------------------------------
pub async fn run_game_with_event_loop(event_loop: EventLoop<()>, config_path: &str) {
    let config = GrootConfig::load(config_path);
    log::info!("Starting Groot Engine: {}", config.project.name);

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

    let mut state: Option<RenderState> = None;
    let mut last_frame_time = Instant::now();

    let _ = event_loop.run(move |event, target| match event {
        Event::Resumed => {
            if state.is_none() {
                let render_ctx = pollster::block_on(RenderContext::new(
                    Arc::clone(&window),
                    config.render.clear_color,
                ));

                let pipeline_3d = Pipeline3D::new(&render_ctx.device, render_ctx.config.format);
                let camera =
                    Camera3D::new(render_ctx.config.width as f32, render_ctx.config.height as f32);

                let initial_camera_uniform = camera.build_uniform();
                let camera_buffer =
                    render_ctx.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Camera Buffer"),
                        contents: bytemuck::cast_slice(&[initial_camera_uniform]),
                        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    });

                let camera_bind_group =
                    render_ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("Camera Bind Group"),
                        layout: &pipeline_3d.camera_bind_group_layout,
                        entries: &[wgpu::BindGroupEntry {
                            binding: 0,
                            resource: camera_buffer.as_entire_binding(),
                        }],
                    });

                let cube_mesh = Mesh::cuboid(&render_ctx.device, 1.0, 1.0, 1.0);
                let mut world = World::new();
                let script_host = GrootScriptHost::new();

                spawn_scene(&mut world, &config.initial_scene);

                state = Some(RenderState {
                    render_ctx,
                    pipeline_3d,
                    camera,
                    camera_buffer,
                    camera_bind_group,
                    cube_mesh,
                    world,
                    script_host,
                });
            } else if let Some(ref mut st) = state {
                if st.render_ctx.size.width > 0 && st.render_ctx.size.height > 0 {
                    st.render_ctx.resize(st.render_ctx.size);
                }
            }
        }
        Event::Suspended => {
            // Surface suspended when app goes into background
        }
        Event::WindowEvent { ref event, window_id } if window_id == window.id() => {
            if let Some(ref mut st) = state {
                st.script_host.input.process_event(event);

                match event {
                    WindowEvent::CloseRequested => target.exit(),
                    WindowEvent::Resized(physical_size) => {
                        st.render_ctx.resize(*physical_size);
                        st.camera.aspect =
                            physical_size.width as f32 / physical_size.height.max(1) as f32;
                    }
                    WindowEvent::RedrawRequested => {
                        let now = Instant::now();
                        let dt = (now - last_frame_time).as_secs_f64();
                        last_frame_time = now;

                        update_scripts(&mut st.script_host, &mut st.world, dt);
                        st.script_host.input.reset_frame_input();

                        let camera_uniform = st.camera.build_uniform();
                        st.render_ctx.queue.write_buffer(
                            &st.camera_buffer,
                            0,
                            bytemuck::cast_slice(&[camera_uniform]),
                        );

                        let output = match st.render_ctx.surface.get_current_texture() {
                            Ok(tex) => tex,
                            Err(wgpu::SurfaceError::Lost) => {
                                st.render_ctx.resize(st.render_ctx.size);
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

                        let view = output
                            .texture
                            .create_view(&wgpu::TextureViewDescriptor::default());
                        let mut encoder = st.render_ctx.device.create_command_encoder(
                            &wgpu::CommandEncoderDescriptor {
                                label: Some("Main Encoder"),
                            },
                        );

                        let model_bind_groups: Vec<(wgpu::BindGroup, wgpu::Buffer)> = st
                            .world
                            .query_mut::<(&Transform3D, &Visual3D)>()
                            .into_iter()
                            .map(|(_id, (tf, vis))| {
                                st.pipeline_3d.create_model_bind_group(
                                    &st.render_ctx.device,
                                    tf.matrix(),
                                    vis.color,
                                )
                            })
                            .collect();

                        {
                            let mut render_pass =
                                encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                    label: Some("3D Render Pass"),
                                    color_attachments: &[Some(
                                        wgpu::RenderPassColorAttachment {
                                            view: &view,
                                            resolve_target: None,
                                            ops: wgpu::Operations {
                                                load: wgpu::LoadOp::Clear(
                                                    st.render_ctx.clear_color,
                                                ),
                                                store: wgpu::StoreOp::Store,
                                            },
                                        },
                                    )],
                                    depth_stencil_attachment: Some(
                                        wgpu::RenderPassDepthStencilAttachment {
                                            view: &st.render_ctx.depth_texture_view,
                                            depth_ops: Some(wgpu::Operations {
                                                load: wgpu::LoadOp::Clear(1.0),
                                                store: wgpu::StoreOp::Store,
                                            }),
                                            stencil_ops: None,
                                        },
                                    ),
                                    occlusion_query_set: None,
                                    timestamp_writes: None,
                                });

                            render_pass.set_pipeline(&st.pipeline_3d.render_pipeline);
                            render_pass.set_bind_group(0, &st.camera_bind_group, &[]);

                            for (bind_group, _buf) in &model_bind_groups {
                                render_pass.set_bind_group(1, bind_group, &[]);
                                render_pass.set_vertex_buffer(
                                    0,
                                    st.cube_mesh.vertex_buffer.slice(..),
                                );
                                render_pass.set_index_buffer(
                                    st.cube_mesh.index_buffer.slice(..),
                                    wgpu::IndexFormat::Uint16,
                                );
                                render_pass.draw_indexed(
                                    0..st.cube_mesh.num_indices,
                                    0,
                                    0..1,
                                );
                            }
                        }

                        st.render_ctx.queue.submit(std::iter::once(encoder.finish()));
                        output.present();
                    }
                    _ => {}
                }
            }
        }
        Event::AboutToWait => {
            window.request_redraw();
        }
        _ => {}
    });
}

use std::collections::HashMap;
use wgpu::util::DeviceExt;

/// Manages wgpu texture bind groups for 2D sprites.
///
/// Besides loading PNGs from the asset store on demand, the manager
/// procedurally generates the exact sprites used by the Groot landing-page
/// flappy-bird mini game and caches them under the same asset paths.
pub struct TextureManager {
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    default_bind_group: wgpu::BindGroup,
    cached_bind_groups: HashMap<String, wgpu::BindGroup>,
}

impl TextureManager {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Texture Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let default_bind_group = Self::create_bind_group_from_rgba(
            device,
            queue,
            &bind_group_layout,
            &sampler,
            1,
            1,
            &[255, 255, 255, 255],
            "Default White Texture",
        );

        let mut cached_bind_groups = HashMap::new();

        // 1. Groot Mascot Bird ("assets/textures/bird.png")
        let mascot_rgba = generate_bird_rgba(128, 128);
        let mascot_bg = Self::create_bind_group_from_rgba(
            device,
            queue,
            &bind_group_layout,
            &sampler,
            128,
            128,
            &mascot_rgba,
            "Groot Mascot Texture",
        );
        cached_bind_groups.insert("assets/textures/bird.png".to_string(), mascot_bg);

        // 2. Neon Green Tech Pipe Sprite ("assets/textures/pipe.png")
        let pipe_rgba = generate_pipe_rgba(64, 256);
        let pipe_bg = Self::create_bind_group_from_rgba(
            device,
            queue,
            &bind_group_layout,
            &sampler,
            64,
            256,
            &pipe_rgba,
            "Neon Pipe Texture",
        );
        cached_bind_groups.insert("assets/textures/pipe.png".to_string(), pipe_bg);

        // 3. Neon Ground Sprite ("assets/textures/ground.png")
        let ground_rgba = generate_ground_rgba(256, 32);
        let ground_bg = Self::create_bind_group_from_rgba(
            device,
            queue,
            &bind_group_layout,
            &sampler,
            256,
            32,
            &ground_rgba,
            "Neon Ground Texture",
        );
        cached_bind_groups.insert("assets/textures/ground.png".to_string(), ground_bg);

        // 3b. Neon Ceiling Sprite ("assets/textures/ceiling.png") - vertical
        // flip of the ground so the neon line sits at the playable edge.
        let ceiling_rgba = generate_ceiling_rgba(256, 32);
        let ceiling_bg = Self::create_bind_group_from_rgba(
            device,
            queue,
            &bind_group_layout,
            &sampler,
            256,
            32,
            &ceiling_rgba,
            "Neon Ceiling Texture",
        );
        cached_bind_groups.insert("assets/textures/ceiling.png".to_string(), ceiling_bg);

        // 4. Faint neon grid background ("assets/textures/grid.png")
        let grid_rgba = generate_grid_rgba(256, 144);
        let grid_bg = Self::create_bind_group_from_rgba(
            device,
            queue,
            &bind_group_layout,
            &sampler,
            256,
            144,
            &grid_rgba,
            "Neon Grid Texture",
        );
        cached_bind_groups.insert("assets/textures/grid.png".to_string(), grid_bg);

        Self {
            bind_group_layout,
            sampler,
            default_bind_group,
            cached_bind_groups,
        }
    }

    fn create_bind_group_from_rgba(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        width: u32,
        height: u32,
        rgba_bytes: &[u8],
        label: &str,
    ) -> wgpu::BindGroup {
        let texture = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            rgba_bytes,
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
    }

    /// Returns the cached bind group for a texture path (if any), the default
    /// white bind group otherwise.
    pub fn get_or_default(&self, path: Option<&str>) -> &wgpu::BindGroup {
        if let Some(p) = path {
            if let Some(bg) = self.cached_bind_groups.get(p) {
                return bg;
            }
        }
        &self.default_bind_group
    }

    /// Loads (and caches) a PNG texture from the asset store, falling back to
    /// the default 1x1 white bind group if the file can't be read or decoded.
    pub fn get_or_load(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        path: &str,
    ) -> &wgpu::BindGroup {
        if self.cached_bind_groups.contains_key(path) {
            return &self.cached_bind_groups[path];
        }

        let loaded = crate::assets::load_asset_bytes(path).and_then(|bytes| {
            let img = match image::load_from_memory(&bytes) {
                Ok(img) => img.to_rgba8(),
                Err(e) => {
                    log::warn!("[GROOT TEXTURE] Failed to decode '{}': {e}", path);
                    return None;
                }
            };
            let (width, height) = img.dimensions();
            let data = img.into_raw();

            let texture = device.create_texture_with_data(
                queue,
                &wgpu::TextureDescriptor {
                    label: Some(path),
                    size: wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                },
                wgpu::util::TextureDataOrder::LayerMajor,
                &data,
            );

            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(path),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            }))
        });

        match loaded {
            Some(bg) => {
                self.cached_bind_groups.insert(path.to_string(), bg);
                &self.cached_bind_groups[path]
            }
            None => &self.default_bind_group,
        }
    }

    pub fn layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }
}

/// Generates the Groot mascot bird sprite exactly as drawn by the landing-page
/// flappy-bird mini game (`Hero.astro`): a near-black `#050705` square with a
/// `#00f076` glowing border and two vertical neon eye bars.
fn generate_bird_rgba(w: usize, h: usize) -> Vec<u8> {
    let mut buf = vec![0u8; w * h * 4];
    // Body rectangle (dark cube face), border ring around it, glow outside.
    let body_lo = 0.13;
    let body_hi = 0.87;
    let ring = 0.06;

    for y in 0..h {
        for x in 0..w {
            let idx = (y * w + x) * 4;
            let fx = x as f32 / w as f32;
            let fy = y as f32 / h as f32;

            // Sprout antenna stem & leaf on top center (tiny 3D-groot nod).
            let dx_sprout = (fx - 0.5).abs();
            if fy < body_lo && dx_sprout < 0.06 {
                if (fy - 0.05).abs() < 0.04 && dx_sprout < 0.015 {
                    // stem
                    buf[idx] = 0;
                    buf[idx + 1] = 230;
                    buf[idx + 2] = 118;
                    buf[idx + 3] = 255;
                    continue;
                }
                if (fy - 0.045).powi(2) + (dx_sprout - 0.05).powi(2) < 0.005
                    || (fy - 0.045).powi(2) + (dx_sprout + 0.05).powi(2) < 0.005
                {
                    // leaves
                    buf[idx] = 0;
                    buf[idx + 1] = 230;
                    buf[idx + 2] = 118;
                    buf[idx + 3] = 255;
                    continue;
                }
            }

            let in_body = fx >= body_lo && fx <= body_hi && fy >= body_lo && fy <= body_hi;
            let in_border = !in_body
                && fx >= body_lo - ring
                && fx <= body_hi + ring
                && fy >= body_lo - ring
                && fy <= body_hi + ring;

            if in_body {
                // Dark metallic cube face (#050705).
                buf[idx] = 5;
                buf[idx + 1] = 7;
                buf[idx + 2] = 5;
                buf[idx + 3] = 255;
            } else if in_border {
                // Neon green border (#00f076).
                buf[idx] = 0;
                buf[idx + 1] = 240;
                buf[idx + 2] = 118;
                buf[idx + 3] = 255;
            } else {
                // Soft green glow fading out from the border edge.
                // Distance from the (border-inclusive) rectangle.
                let b_lo = body_lo - ring;
                let b_hi = body_hi + ring;
                let dx = (b_lo - fx).max(fx - b_hi).max(0.0);
                let dy = (b_lo - fy).max(fy - b_hi).max(0.0);
                let dist = dx.max(dy);
                let glow = (1.0 - dist / 0.08).clamp(0.0, 1.0);
                buf[idx] = 0;
                buf[idx + 1] = 240;
                buf[idx + 2] = 118;
                buf[idx + 3] = (60.0 * glow) as u8;
            }
        }
    }

    // Eyes: two vertical neon bars, slightly above center (matches site's
    // `fillRect(-6, -5, 4, 8)` / `fillRect(2, -5, 4, 8)` on a 28px sprite).
    for y in 0..h {
        for x in 0..w {
            let idx = (y * w + x) * 4;
            let fx = x as f32 / w as f32;
            let fy = y as f32 / h as f32;

            let left_eye_x = fx >= 0.35 && fx <= 0.42;
            let right_eye_x = fx >= 0.58 && fx <= 0.65;
            let eye_y = fy >= 0.38 && fy <= 0.60;

            if eye_y && (left_eye_x || right_eye_x) {
                buf[idx] = 0;
                buf[idx + 1] = 240;
                buf[idx + 2] = 118;
                buf[idx + 3] = 255;
            }
        }
    }

    buf
}

/// Generates high-tech neon green pipes exactly like the landing page: a dark
/// `#0a140d` body with a `#00f076` border and green glow.
fn generate_pipe_rgba(w: usize, h: usize) -> Vec<u8> {
    let mut buf = vec![0u8; w * h * 4];
    let border_px = 3usize;
    let glow_px = 9usize;

    for y in 0..h {
        for x in 0..w {
            let idx = (y * w + x) * 4;

            let dx = x.min(w - 1 - x);
            let dy = y.min(h - 1 - y);
            let edge_dist = dx.min(dy);

            if edge_dist < border_px {
                buf[idx] = 0;
                buf[idx + 1] = 240;
                buf[idx + 2] = 118;
                buf[idx + 3] = 255;
                continue;
            }

            if edge_dist < border_px + glow_px {
                let glow = 1.0 - ((edge_dist - border_px) as f32 / glow_px as f32);
                buf[idx] = 0;
                buf[idx + 1] = 240;
                buf[idx + 2] = 118;
                buf[idx + 3] = (30.0 * glow) as u8;
                continue;
            }

            // Solid dark pipe body (#0a140d).
            buf[idx] = 10;
            buf[idx + 1] = 20;
            buf[idx + 2] = 13;
            buf[idx + 3] = 255;
        }
    }
    buf
}

/// Generates neon ground: a bright `#00f076` glowing top line over a dark
/// slate base. The neon line occupies the top `line_frac` of the texture so it
/// stays thin (in world units) even when the quad is stretched to cover the
/// whole screen edge.
fn generate_ground_rgba(w: usize, h: usize) -> Vec<u8> {
    let mut buf = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let idx = (y * w + x) * 4;
            let fy = y as f32 / h as f32;

            // Bright glowing top line.
            if fy < 0.06 {
                buf[idx] = 0;
                buf[idx + 1] = 230;
                buf[idx + 2] = 118;
                buf[idx + 3] = 255;
            } else if fy < 0.12 {
                // soft glow falloff (fully opaque)
                buf[idx] = 0;
                buf[idx + 1] = 240;
                buf[idx + 2] = 118;
                buf[idx + 3] = 255;
            } else {
                // Dark slate base (fully opaque)
                buf[idx] = 14;
                buf[idx + 1] = 20;
                buf[idx + 2] = 28;
                buf[idx + 3] = 255;
            }
        }
    }
    buf
}

/// Generates the ceiling as a vertical flip of the ground so the neon line
/// sits at the bottom edge (the playable boundary).
fn generate_ceiling_rgba(w: usize, h: usize) -> Vec<u8> {
    let mut buf = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let idx = (y * w + x) * 4;
            let fy = y as f32 / h as f32;

            // Bright glowing bottom line.
            if fy > 0.94 {
                buf[idx] = 0;
                buf[idx + 1] = 230;
                buf[idx + 2] = 118;
                buf[idx + 3] = 255;
            } else if fy > 0.88 {
                // soft glow falloff (fully opaque)
                buf[idx] = 0;
                buf[idx + 1] = 240;
                buf[idx + 2] = 118;
                buf[idx + 3] = 255;
            } else {
                // Dark slate base (fully opaque)
                buf[idx] = 14;
                buf[idx + 1] = 20;
                buf[idx + 2] = 28;
                buf[idx + 3] = 255;
            }
        }
    }
    buf
}

/// Generates the faint tech grid from the landing page
/// (`rgba(0, 240, 118, 0.04)` lines every 30px).
fn generate_grid_rgba(w: usize, h: usize) -> Vec<u8> {
    let mut buf = vec![0u8; w * h * 4];
    // ~30px spacing scaled to the texture size (grid spans the full canvas).
    let cell = 30.0 * (w as f32 / 700.0);
    let spacing = cell.max(8.0);
    for y in 0..h {
        for x in 0..w {
            let idx = (y * w + x) * 4;
            let on_v = (x as f32 % spacing) < 1.0;
            let on_h = (y as f32 % spacing) < 1.0;
            if on_v || on_h {
                buf[idx] = 0;
                buf[idx + 1] = 240;
                buf[idx + 2] = 118;
                buf[idx + 3] = 12;
            } else {
                buf[idx] = 0;
                buf[idx + 1] = 0;
                buf[idx + 2] = 0;
                buf[idx + 3] = 0;
            }
        }
    }
    buf
}

#[cfg(test)]
mod verify_tests {
    use super::*;

    #[test]
    fn verify_sprite_pixels() {
        // Bird: border green, interior dark, eyes green
        let b = generate_bird_rgba(128, 128);
        let px = |x: usize, y: usize| (y * 128 + x) * 4;
        let border = (b[px(10, 64) + 0], b[px(10, 64) + 1], b[px(10, 64) + 2], b[px(10, 64) + 3]);
        let interior = (b[px(64, 80) + 0], b[px(64, 80) + 1], b[px(64, 80) + 2]);
        let eye = (b[px(49, 60) + 0], b[px(49, 60) + 1], b[px(49, 60) + 2]);
        let corner_glow = b[px(2, 2) + 3];
        assert_eq!(border, (0, 240, 118, 255), "neon green border, opaque");
        assert_eq!(interior, (5, 7, 5), "dark face");
        assert_eq!(eye, (0, 240, 118), "neon eyes");
        assert!(corner_glow > 0 && corner_glow < 255, "fading glow");

        // Pipe: border green, body dark
        let p = generate_pipe_rgba(64, 256);
        let ppx = |x: usize, y: usize| (y * 64 + x) * 4;
        let pborder = (p[ppx(1, 128) + 0], p[ppx(1, 128) + 1], p[ppx(1, 128) + 2]);
        let pbody = (p[ppx(32, 128) + 0], p[ppx(32, 128) + 1], p[ppx(32, 128) + 2]);
        assert_eq!(pborder, (0, 240, 118));
        assert_eq!(pbody, (10, 20, 13));

        // Ground: top line green, base dark
        let g = generate_ground_rgba(256, 32);
        let gpx = |x: usize, y: usize| (y * 256 + x) * 4;
        let gtop = (g[gpx(128, 1) + 0], g[gpx(128, 1) + 1], g[gpx(128, 1) + 2]);
        let gbase = (g[gpx(128, 30) + 0], g[gpx(128, 30) + 1], g[gpx(128, 30) + 2], g[gpx(128, 30) + 3]);
        let gglow = g[gpx(128, 3) + 3];
        assert_eq!(gtop, (0, 230, 118));
        assert_eq!(gbase, (14, 20, 28, 255));
        assert_eq!(gglow, 255, "ground glow must be fully opaque");

        // Ceiling: bottom line green, base dark
        let c = generate_ceiling_rgba(256, 32);
        let cpx = |x: usize, y: usize| (y * 256 + x) * 4;
        let cbot = (c[cpx(128, 31) + 0], c[cpx(128, 31) + 1], c[cpx(128, 31) + 2]);
        let cbase = (c[cpx(128, 1) + 0], c[cpx(128, 1) + 1], c[cpx(128, 1) + 2], c[cpx(128, 1) + 3]);
        let cglow = c[cpx(128, 29) + 3];
        assert_eq!(cbot, (0, 230, 118));
        assert_eq!(cbase, (14, 20, 28, 255));
        assert_eq!(cglow, 255, "ceiling glow must be fully opaque");

        // Grid: has both transparent and faint-green pixels
        let gr = generate_grid_rgba(256, 144);
        let mut alpha_max = 0u8;
        for i in (3..gr.len()).step_by(4) {
            alpha_max = alpha_max.max(gr[i]);
        }
        assert!(alpha_max > 0 && alpha_max <= 16, "faint grid");
    }
}

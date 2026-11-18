# 03 — Host-Guest HAL ABI (`ube:graphics`)

The Groot guest WebAssembly module interacts with the native Rust host via strictly typed WIT interfaces defined in `wit/groot.wit`:

```wit
package ube:graphics@0.2.0;

interface window {
    create-window: func(width: u32, height: u32, title: string);
}

interface gpu {
    setup: func(width: u32, height: u32);
    set-vertices: func(data: list<f32>);
    draw-frame: func(r: f64, g: f64, b: f64);
}

world groot-runner {
    import window;
    import gpu;
    include wasi:cli/command@0.2.0;
}
```

## ABI Calling Flow
1. Guest invokes `platform::open_window(width, height, title)`:
   - Passes string pointer and length through Canonical ABI lowering.
   - Host initializes the `winit` window and attaches a `wgpu` surface.
2. Guest fills a batch buffer and calls `platform::submit(vertices)`:
   - Slices `[]f32` map directly to linear memory addresses.
   - Host copies vertex slice directly to the staging GPU vertex buffer.
3. Guest calls `platform::present(r, g, b)`:
   - Host issues an instanced draw call and swaps the backbuffer.

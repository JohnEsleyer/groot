/// Android native entry point.
///
/// `android-activity`'s `native_app_glue` invokes this symbol on a dedicated
/// thread once the APK's NativeActivity is created. We attach the `AndroidApp`
/// to a winit event loop and hand off to the shared async runner.
#[cfg(target_os = "android")]
#[no_mangle]
pub fn android_main(app: android_activity::AndroidApp) {
    use winit::platform::android::EventLoopBuilderExtAndroid;

    crate::platform::init_platform_logging();

    let mut builder = winit::event_loop::EventLoopBuilder::new();
    builder.with_android_app(app);
    let event_loop = builder.build().expect("Failed to create Android EventLoop");

    log::info!("Starting Groot Engine on Android");
    pollster::block_on(crate::run_game_with_event_loop(
        event_loop,
        "assets/config.ron",
    ));
}
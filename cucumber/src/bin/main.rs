use std::path::PathBuf;

use clap::Parser;

use cucumber::ui::MyApp;

pub const APP_ID: &str = "cucumber";

/// Bitwig theme editor GUI
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Input JAR
    jar_in: Option<PathBuf>,

    /// Output JAR
    jar_out: Option<PathBuf>,
}

// When compiling natively:
#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    use tracing_subscriber;
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    let options = eframe_options();
    eframe::run_native(
        "Cucumber",
        options,
        Box::new(|cc| {
            let ctx = cc.egui_ctx.clone();
            let app = MyApp::new(ctx, args.jar_in, args.jar_out)?;
            Ok(Box::new(app))
        }),
    )
}

pub fn eframe_options() -> eframe::NativeOptions {
    let os = eframe::egui::os::OperatingSystem::default();
    eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_app_id(APP_ID) // Controls where on disk the app state is persisted
            .with_decorations(!re_ui::CUSTOM_WINDOW_DECORATIONS) // Maybe hide the OS-specific "chrome" around the window
            .with_fullsize_content_view(re_ui::fullsize_content(os))
            .with_icon(icon_data())
            .with_inner_size([1200.0, 720.0])
            .with_min_inner_size([320.0, 450.0]) // Should be high enough to fit the rerun menu
            .with_title_shown(!re_ui::fullsize_content(os))
            .with_titlebar_buttons_shown(!re_ui::CUSTOM_WINDOW_DECORATIONS)
            .with_titlebar_shown(!re_ui::fullsize_content(os))
            .with_transparent(re_ui::CUSTOM_WINDOW_DECORATIONS), // To have rounded corners without decorations we need transparency

        renderer: eframe::Renderer::Wgpu,
        depth_buffer: 0,
        multisampling: 0,

        ..Default::default()
    }
}

#[allow(clippy::unnecessary_wraps)]
fn icon_data() -> eframe::egui::IconData {
    cfg_if::cfg_if! {
        if #[cfg(target_os = "macos")] {
            let app_icon_png_bytes = include_bytes!("../../assets/icon.png");
        } else if #[cfg(target_os = "windows")] {
            let app_icon_png_bytes = include_bytes!("../../assets/icon.png");
        } else {
            let app_icon_png_bytes = include_bytes!("../../assets/icon.png");
        }
    };

    // We include the .png with `include_bytes`. If that fails, things are extremely broken.
    match eframe::icon_data::from_png_bytes(app_icon_png_bytes) {
        Ok(icon_data) => icon_data,
        Err(err) => {
            #[cfg(debug_assertions)]
            panic!("Failed to load app icon: {err}");

            #[cfg(not(debug_assertions))]
            {
                use tracing::warn;

                warn!("Failed to load app icon: {err}");
                Default::default()
            }
        }
    }
}

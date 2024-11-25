use clap::Parser;

use cucumber::ui::MyApp;

/// Bitwig theme editor GUI
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Input JAR
    jar_in: Option<String>,

    /// Output JAR
    jar_out: Option<String>,
}

// When compiling natively:
#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    use eframe::egui;

    let args = Args::parse();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 720.0])
            .with_position([400.0, 150.0]),
        ..Default::default()
    };
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

// When compiling to web using trunk:
#[cfg(target_arch = "wasm32")]
fn main() {
    use eframe::wasm_bindgen::JsCast as _;

    // Redirect `log` message to `console.log` and friends:
    eframe::WebLogger::init(log::LevelFilter::Debug).ok();

    let web_options = eframe::WebOptions::default();

    wasm_bindgen_futures::spawn_local(async {
        let document = web_sys::window()
            .expect("No window")
            .document()
            .expect("No document");

        let canvas = document
            .get_element_by_id("the_canvas_id")
            .expect("Failed to find the_canvas_id")
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .expect("the_canvas_id was not a HtmlCanvasElement");

        let start_result = eframe::WebRunner::new()
            .start(
                canvas,
                web_options,
                // Box::new(|cc| Ok(Box::new(eframe_template::TemplateApp::new(cc)))),
                Box::new(|_| Ok(Box::new(MyApp::new(None, None)?))),
            )
            .await;

        // Remove the loading text and spinner:
        if let Some(loading_text) = document.get_element_by_id("loading_text") {
            match start_result {
                Ok(_) => {
                    loading_text.remove();
                }
                Err(e) => {
                    loading_text.set_inner_html(
                        "<p> The app has crashed. See the developer console for details. </p>",
                    );
                    panic!("Failed to start eframe: {e:?}");
                }
            }
        }
    });
}

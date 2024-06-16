use std::collections::HashMap;
use std::io::{self, Cursor, Read, Seek, SeekFrom};

use leptos::SignalGet;
use leptos::{component, create_node_ref, create_signal, html::Div, logging, view, IntoView};
use leptos::For;
use leptos_use::{use_drop_zone_with_options, UseDropZoneEvent, UseDropZoneOptions, UseDropZoneReturn};
use cucumber::{extract_general_goodies, GeneralGoodies};

struct VecReader {
    cursor: Cursor<Vec<u8>>,
}

impl VecReader {
    // Create a new VecReader from a Vec<u8>
    pub fn new(data: Vec<u8>) -> Self {
        VecReader {
            cursor: Cursor::new(data),
        }
    }
}

// Implement the Read trait for VecReader
impl Read for VecReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.cursor.read(buf)
    }
}

// Implement the Seek trait for VecReader
impl Seek for VecReader {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.cursor.seek(pos)
    }
}

fn handle_jar_blob(data: Vec<u8>) -> GeneralGoodies {
    logging::log!("STG 1");
    let reader = VecReader::new(data);
    logging::log!("STG 2");
    let mut zip = zip::ZipArchive::new(reader).unwrap();
    logging::log!("STG 3");
    extract_general_goodies(&mut zip).unwrap()
}

#[component]
pub fn Editor() -> impl IntoView {
    let drop_zone_el = create_node_ref::<Div>();
    let (colors, set_colors) = create_signal(vec![]);
    let (known_colors, set_known_colors) = create_signal(HashMap::new());

    let on_drop = move |mut event: UseDropZoneEvent| {
        logging::log!("DROP: {:?}", event);
        let file = event.files.pop().unwrap();

        // #[cfg(not(feature = "ssr"))]
        {
            use web_sys::FileReader;
            use wasm_bindgen::closure::Closure;
            use wasm_bindgen::JsCast;
            use web_sys::Event;

            let reader = FileReader::new().unwrap();
            let onloadend = Closure::wrap(Box::new(move |event: Event| {
                let reader: FileReader = event.target().unwrap().unchecked_into();
                if reader.ready_state() == FileReader::DONE {
                    let result = reader.result().unwrap();
                    let array = js_sys::Uint8Array::new(&result);
                    let bytes = array.to_vec();
                    // Process the bytes as needed
                    logging::log!("Read {} bytes", bytes.len());
                    let goodies = handle_jar_blob(bytes);
                    set_known_colors(goodies.named_colors.iter().map(|c| (c.color_name.clone(), c.components.clone())).collect());
                    set_colors(goodies.named_colors);
                }
            }) as Box<dyn FnMut(_)>);

            reader.set_onloadend(Some(onloadend.as_ref().unchecked_ref()));
            reader.read_as_array_buffer(&file).unwrap();
            onloadend.forget();
        }
    };

    let UseDropZoneReturn {
        is_over_drop_zone,
        ..
    } = use_drop_zone_with_options(
        drop_zone_el,
        UseDropZoneOptions::default().on_drop(on_drop)
    );

    view! {
        <h1>"Editor"</h1>

        <div
            class:dropover=is_over_drop_zone
            node_ref=drop_zone_el
        >
            "Drop JAR here"
        </div>
        <h2>"Colors"</h2>
        <div class="colors">
            <For
                each=colors
                key=|c| c.color_name.clone()
                let:color
            >   {
                    let (r, g, b) = color.components.to_rgb(&known_colors.get());
                    let a = color.components.alpha().unwrap_or(255) as f32 / 255.0;
                    let bg = format!("rgba({r}, {g}, {b}, {a})");
                    let fg = if (r as u16 + g as u16 + b as u16 + ((255.0 - a * 255.0) * 2.0) as u16) > 128 * 3 {
                        "black"
                    } else {
                        "white"
                    };

                    view! { <div
                        class="color"
                        style:background-color=bg
                        style:color=fg
                    >
                        { color.color_name }
                    </div> }
                }
            </For>
        </div>
    }
}
use std::io::Cursor;

use cucumber::types::{AbsoluteColor, CucumberBitwigTheme};
use leptos::{create_resource, create_signal, ServerFnError};
use leptos::{component, create_node_ref, html::Div, logging, view, IntoView, server};
use leptos_use::{use_drop_zone_with_options, UseDropZoneEvent, UseDropZoneOptions, UseDropZoneReturn};

use leptos::Suspense;
use leptos::SignalUpdate;
use cucumber::types::NamedColor;
use crate::components::color_editor::ColorEditor;

fn handle_jar_blob(data: Vec<u8>) -> CucumberBitwigTheme {
    logging::log!("STG 1");
    let reader = Cursor::new(data);
    logging::log!("STG 2");
    let mut zip = zip::ZipArchive::new(reader).unwrap();
    logging::log!("STG 3");
    CucumberBitwigTheme::from_jar(&mut zip)
}

#[server(GetTheme, "/api")]
pub async fn get_theme(theme_name: String) -> Result<CucumberBitwigTheme, ServerFnError> {
    // TODO: Make this secure (disallow fs path injection)
    let text = tokio::fs::read_to_string(format!("storage/{}.json", theme_name)).await?;
    let theme: CucumberBitwigTheme = serde_json::from_str(&text).unwrap();
    Ok(theme)
}

#[derive(Debug, Clone)]
pub struct CurrentColor {
    pub name: String,
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

#[component]
pub fn Editor() -> impl IntoView {
    let drop_zone_el = create_node_ref::<Div>();

    // our resource
    let async_data = create_resource(
        || {},
        // every time `count` changes, this will run
        move |_| async move {
            get_theme("factory-theme".into()).await
        },
    );

    let (current_color, set_current_color) = create_signal(None::<CurrentColor>);

    let on_drop = move |mut event: UseDropZoneEvent| {
        logging::log!("DROP: {:?}", event);
        let file = event.files.pop().unwrap();

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
                    let theme = handle_jar_blob(bytes);
                    async_data.update(|old_theme| {
                        *old_theme = Some(Ok(theme));
                    });
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

    let on_click = move |event| {
        async_data.update(|theme| {
            if let Some(Ok(theme)) = theme {
                theme.name = "Tirsetiarsentoiarsent".into();
            }
        });
    };

    view! {
        <h1>"Editor"</h1>

        <ColorEditor maybe_color=current_color set_current_color=set_current_color/>

        <button on:click=on_click>"MUTATE"</button>

        <Suspense
            fallback=move || view! { <span>"Not ready"</span> }
        >
            <h2>"Loaded data:"</h2>
            {move || {
                async_data.and_then(|theme| view! { <pre> { format!("{:#?}", theme.name) } </pre> })
            }}
        </Suspense>

        <div
            class:dropover=is_over_drop_zone
            node_ref=drop_zone_el
        >
            "Drop JAR here"
        </div>
        <h2>"Colors"</h2>
        <Suspense
            fallback=move || view! { <span>"Not ready"</span> }
        >
            <div class="colors">
                { move || {
                    async_data.and_then(|theme| {
                        theme.named_colors.iter().map(|(name, color)| {
                            match color {
                                NamedColor::Absolute(AbsoluteColor { r, g, b, a }) => {
                                    let r = *r;
                                    let g = *g;
                                    let b = *b;
                                    let a = *a;
                                    let a_u8 = a;
                                    let a = a as f32 / 255.0;
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
                                        on:click=move |_| {
                                            logging::log!("CLIIIIICK");
                                            set_current_color(
                                                Some(CurrentColor { name: "Abstract Button Pressed Background".into(), r, g, b, a: a_u8 })
                                            );
                                        }
                                    >
                                        { name }
                                    </div> }
                                },
                                NamedColor::Relative(_) => view! {
                                    <div class="color">
                                        { name }" (RELATIVE - IGNORED)"
                                    </div>
                                }
                            }
                        }).collect::<Vec<_>>()
                    })
                } }
            </div>
        </Suspense>
    }
}
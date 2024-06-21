use leptos::{component, view, IntoView, ReadSignal, WriteSignal};
use leptos::SignalGet;

use leptos::event_target_value;

use super::editor::CurrentColor;

#[component]
pub fn ColorEditor(
    maybe_color: ReadSignal<Option<CurrentColor>>,
    set_current_color: WriteSignal<Option<CurrentColor>>,
) -> impl IntoView {

    let set_color = move |r| {
        let color = maybe_color.get().unwrap();
        set_current_color(Some(CurrentColor { name: color.name, r: r, g: color.g, b: color.b, a: color.a }))
    };

    view! {
        <div>
            <h3>"COLOR EDITOR:" { move || {
                let color = maybe_color.get();
                if let Some(color) = color {
                    view! {
                        <div>
                            <input
                                type="range"
                                min="0"
                                max="255"
                                prop:value=color.r
                                on:input=move |e| {
                                    let new_value: u8 = event_target_value(&e).parse().unwrap();
                                    set_color(new_value);
                                }
                            />
                            { format!("{:?}", color) }
                        </div>
                    }.into_view()
                } else {
                    view! {
                        <span> "INACTIVE" </span>
                    }.into_view()
                }
            } }</h3>
        </div>
    }
}
// templates/rust_leptos_csr_v1/src/lib.rs
use leptos::prelude::*;
use wasm_bindgen::prelude::*;

// Placeholder for component name
#[component]
fn PlaceholderAppComponent() -> impl IntoView {
    let (count, set_count) = signal(0);

    view! {
        <h1>"Welcome to Leptos!"</h1>
        <p>"Page Title Placeholder: Leptos Placeholder App"</p>
        <button on:click=move |_| set_count.update(|n| *n += 1)>
            "Click Me: " {count}
        </button>
    }
}

// Mount the app to the body
#[wasm_bindgen(start)]
pub fn main() {
    // Set panic hook for better error messages in browser console
    console_error_panic_hook::set_once();
    _ = leptos::mount::mount_to_body(PlaceholderAppComponent); // Use placeholder name
}
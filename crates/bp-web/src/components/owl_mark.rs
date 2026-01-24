use leptos::prelude::*;
#[cfg(feature = "hydrate")]
use wasm_bindgen::JsCast;

const SHADES: &[char] = &['\u{2592}', '\u{2591}']; // ▒ and ░
const SEGMENT: &str = " - - - "; // 3 dashes with spaces

#[cfg(feature = "hydrate")]
fn get_random_shade() -> char {
    let idx = (js_sys::Math::random() * SHADES.len() as f64) as usize;
    SHADES[idx.min(SHADES.len() - 1)]
}

#[cfg(not(feature = "hydrate"))]
fn get_random_shade() -> char {
    // On server, just alternate
    SHADES[0]
}

fn generate_initial_line(length: usize) -> String {
    let mut line = String::with_capacity(length + 10);
    while line.len() < length {
        line.push(get_random_shade());
        line.push_str(SEGMENT);
    }
    line
}

/// Check if user prefers reduced motion (client-side only)
#[cfg(feature = "hydrate")]
fn prefers_reduced_motion() -> bool {
    web_sys::window()
        .and_then(|w| w.match_media("(prefers-reduced-motion: reduce)").ok())
        .flatten()
        .map(|mq| mq.matches())
        .unwrap_or(false)
}

/// Animated line component that scrolls ASCII characters
#[component]
fn AnimatedLine() -> impl IntoView {
    #[cfg(feature = "hydrate")]
    let (line, set_line) = signal(generate_initial_line(50));
    #[cfg(not(feature = "hydrate"))]
    let line = signal(generate_initial_line(50)).0;

    // Only run animation on client
    #[cfg(feature = "hydrate")]
    {
        let reduced_motion = prefers_reduced_motion();

        Effect::new(move |_| {
            if reduced_motion {
                return;
            }

            let window = web_sys::window().expect("no window");
            let callback = wasm_bindgen::closure::Closure::wrap(Box::new(move || {
                set_line.update(|prev| {
                    if prev.len() > 1 {
                        prev.remove(0);
                    }
                    if prev.len() < 50 {
                        prev.push(get_random_shade());
                        prev.push_str(SEGMENT);
                    }
                });
            }) as Box<dyn FnMut()>);

            let _ =
                window.set_interval_with_callback_and_timeout_and_arguments_0(callback.as_ref().unchecked_ref(), 400);

            callback.forget();
        });
    }

    let display_line = move || {
        let l = line.get();
        l.chars().take(20).collect::<String>()
    };

    view! {
        <span class="text-[var(--ink-light)]">{display_line}</span>
    }
}

/// Animated gradient dash border with title
#[component]
pub fn AnimatedGradientDashBorder(#[prop(into)] title: String) -> impl IntoView {
    view! {
        <div class="select-none overflow-hidden whitespace-nowrap flex justify-center items-center">
            <AnimatedLine />
            <span class="font-bold px-6">{title}</span>
            <AnimatedLine />
        </div>
    }
}

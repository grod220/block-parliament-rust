use leptos::prelude::*;
use wasm_bindgen::prelude::*;
use web_sys::window;

const SHADES: &[char] = &['\u{2592}', '\u{2591}']; // and
const SEGMENT: &str = " - - - "; // 3 dashes with spaces

fn get_random_shade() -> char {
    let idx = (js_sys::Math::random() * SHADES.len() as f64) as usize;
    SHADES[idx.min(SHADES.len() - 1)]
}

fn generate_initial_line(length: usize) -> String {
    let mut line = String::with_capacity(length + 10);
    while line.len() < length {
        line.push(get_random_shade());
        line.push_str(SEGMENT);
    }
    line
}

/// Check if user prefers reduced motion
fn prefers_reduced_motion() -> bool {
    window()
        .and_then(|w| w.match_media("(prefers-reduced-motion: reduce)").ok())
        .flatten()
        .map(|mq| mq.matches())
        .unwrap_or(false)
}

/// Animated line component that scrolls ASCII characters
#[component]
fn AnimatedLine() -> impl IntoView {
    let (line, set_line) = signal(generate_initial_line(50));
    let reduced_motion = prefers_reduced_motion();

    // Only start animation if reduced motion is not preferred
    Effect::new(move |_| {
        if reduced_motion {
            return;
        }

        // Use web_sys directly for setInterval since gloo's Interval isn't Send+Sync
        let window = web_sys::window().expect("no window");
        let callback = Closure::wrap(Box::new(move || {
            set_line.update(|prev| {
                // Shift left by removing first char
                if prev.len() > 1 {
                    prev.remove(0);
                }
                // Add new content if needed
                if prev.len() < 50 {
                    prev.push(get_random_shade());
                    prev.push_str(SEGMENT);
                }
            });
        }) as Box<dyn FnMut()>);

        let _ = window.set_interval_with_callback_and_timeout_and_arguments_0(callback.as_ref().unchecked_ref(), 400);

        // Keep callback alive - in a real app you'd want to clear this on cleanup
        callback.forget();
    });

    // Display first 20 chars
    let display_line = move || {
        let l = line.get();
        l.chars().take(20).collect::<String>()
    };

    view! {
        <span class="text-[var(--ink-light)]">{display_line}</span>
    }
}

/// Animated gradient dash border with title
/// Ported from OwlMark.tsx
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

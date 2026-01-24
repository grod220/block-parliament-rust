use leptos::prelude::*;

/// Section component - wrapper with decorative ASCII border
/// Ported from Section.tsx
#[component]
pub fn Section(#[prop(into)] id: String, #[prop(into)] title: String, children: Children) -> impl IntoView {
    view! {
        <section id=id class="mb-8">
            <h2 class="font-bold uppercase mb-3">
                {format!("─┤ {} ├─", title)}
            </h2>
            <div class="pl-4 border-l border-dashed border-[var(--rule)]">
                {children()}
            </div>
        </section>
    }
}

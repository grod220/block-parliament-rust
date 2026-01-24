use leptos::prelude::*;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::path;

use crate::pages::{HomePage, SecurityPage};

#[component]
pub fn App() -> impl IntoView {
    view! {
        <Router>
            <Routes fallback=|| view! { <p>"404 - Page not found"</p> }>
                <Route path=path!("/") view=HomePage />
                <Route path=path!("/security") view=SecurityPage />
            </Routes>
        </Router>
    }
}

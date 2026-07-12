#![allow(non_snake_case)]
//! App shell: router + shared styles. Pages live in `crate::pages`.

use dioxus::prelude::*;

use crate::pages::login::Login;
use crate::pages::orders::Orders;
use crate::pages::register::Register;

#[derive(Routable, Clone, PartialEq)]
pub enum Route {
    #[route("/")]
    Orders {},
    #[route("/login")]
    Login {},
    #[route("/register")]
    Register {},
}

pub fn App() -> Element {
    rsx! {
        style { {CSS} }
        dioxus_clerk::ClerkProvider { publishable_key: env!("CLERK_PUBLISHABLE_KEY"),
            Router::<Route> {}
        }
    }
}

pub fn status_class(status: &str) -> &'static str {
    match status {
        "fulfilled" => "pill ok",
        "failed" => "pill err",
        "queued" => "pill",
        _ => "pill wait", // validating / charging / fulfilling
    }
}

pub const CSS: &str = r#"
:root { color-scheme: light dark; }
* { box-sizing: border-box; }
body { margin: 0; }
.wrap { max-width: 820px; margin: 0 auto; padding: 2rem 1.25rem;
  font: 15px/1.5 system-ui, -apple-system, Segoe UI, Roboto, sans-serif; }
h1 { margin: 0; font-size: 1.9rem; letter-spacing: -0.02em; }
.sub { margin: .25rem 0 1.5rem; opacity: .7; }
.card { border: 1px solid color-mix(in srgb, currentColor 15%, transparent);
  border-radius: 12px; padding: 1.1rem 1.25rem; margin-bottom: 1.25rem; }
.card h2 { margin: 0 0 .8rem; font-size: 1.05rem; }
.row { display: flex; gap: .6rem; flex-wrap: wrap; }
.col { display: flex; flex-direction: column; gap: .6rem; }
input { flex: 1 1 140px; padding: .55rem .7rem; border-radius: 8px;
  border: 1px solid color-mix(in srgb, currentColor 25%, transparent);
  background: transparent; color: inherit; }
button { padding: .55rem .9rem; border-radius: 8px; border: 0; cursor: pointer;
  font-weight: 600; }
.primary { background: #4f46e5; color: #fff; }
.ghost { background: transparent; border: 1px solid
  color-mix(in srgb, currentColor 25%, transparent); color: inherit; }
table { width: 100%; border-collapse: collapse; }
th, td { text-align: left; padding: .55rem .5rem; border-bottom:
  1px solid color-mix(in srgb, currentColor 12%, transparent); vertical-align: middle; }
th { font-size: .78rem; text-transform: uppercase; letter-spacing: .05em; opacity: .6; }
.mono { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: .82rem; }
.muted { opacity: .5; }
.err { color: #dc2626; }
.pill { display: inline-block; padding: .18rem .55rem; border-radius: 999px;
  font-size: .8rem; font-weight: 600;
  background: color-mix(in srgb, currentColor 12%, transparent); }
.pill.ok { background: #16a34a22; color: #16a34a; }
.pill.err { background: #dc262622; color: #dc2626; }
.pill.wait { background: #4f46e522; color: #6366f1; }
.nav { display: flex; align-items: center; justify-content: space-between;
  margin-bottom: 1.25rem; gap: .75rem; }
.nav .who { opacity: .7; font-size: .9rem; }
.narrow { max-width: 420px; }
a { color: #6366f1; }
"#;

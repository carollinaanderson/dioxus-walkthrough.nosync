# Chapter 1 — Hello, Dioxus

## What you'll learn

[Dioxus](https://dioxuslabs.com) lets you write a UI as Rust functions that
return `rsx!` markup, and it compiles to WebAssembly so it runs in a normal
browser tab. In this chapter you'll get the smallest possible Dioxus app
running, and see how a *component* (`App`) and `rsx!` fit together.

There's no server, no database, nothing async yet — just enough to prove the
toolchain works end to end.

## Run it

From this directory:

```bash
dx serve
```

Open the URL it prints (usually `http://localhost:8080`). You should see a
"MyApp" heading and a little card. Edit the text in `src/app.rs` and save —
`dx serve` hot-reloads the page automatically.

## How it works

- `src/main.rs` calls `dioxus::launch(App)` — this boots the Dioxus web
  renderer and mounts the `App` component into the page.
- `src/app.rs` defines `App`, a function that returns `Element`. The body
  uses the `rsx!` macro, which looks like HTML but is regular Rust under the
  hood — you can mix in `{expressions}` anywhere.
- The `style { {CSS} }` line just injects a plain CSS string into the page.
  We'll reuse that same stylesheet for the rest of the tutorial, so you won't
  need to touch CSS again after this chapter.

That's the whole app. Nothing here talks to a network or a database — it's a
static page, same as if you'd written it in JavaScript with React.

## Your turn: get to chapter 2

Chapter 2 adds a **server**: a real backend process, and a `#[server]`
function the browser calls over HTTP. Here's how to build that yourself,
starting from this chapter's code, step by step.

1. **Copy this chapter as your working copy** (don't edit chapters/01 in
   place — keep it as a clean reference):

   ```bash
   cp -r ../01-hello-dioxus ../my-02-server-functions
   cd ../my-02-server-functions
   ```

2. **Turn on Dioxus's `fullstack` feature and add server-only dependencies.**
   Dioxus apps that have a server are compiled *twice*: once to WebAssembly
   for the browser, once to a native binary for the server. Cargo features
   let one `Cargo.toml` describe both builds — `web` pulls in the browser
   renderer, `server` pulls in `dioxus/server` plus the native HTTP stack.
   Replace your `Cargo.toml`'s `[dependencies]` section with:

   ```toml
   [dependencies]
   dioxus = { version = "0.7", features = ["fullstack"] }

   # Server-only — not compiled into the WASM bundle.
   axum = { version = "0.8", optional = true }
   tokio = { version = "1", features = ["full"], optional = true }

   [features]
   default = ["web"]
   web = ["dioxus/web"]
   server = ["dioxus/server", "dep:axum", "dep:tokio"]
   ```

   `axum` and `tokio` are marked `optional = true` — that's what makes them
   "server-only": Cargo won't compile them in unless the `server` feature is
   on, and the `server` feature is off by default (`default = ["web"]`).
   `dx serve` knows to build both feature sets for a fullstack app, so you
   don't invoke this manually — it does it for you.

3. **Branch `main.rs` on which build you're in.** Replace your `fn main()`
   with:

   ```rust
   mod app;
   mod server;
   use app::App;

   fn main() {
       // Client entrypoint: compiled to WASM, runs in the browser.
       #[cfg(not(feature = "server"))]
       dioxus::launch(App);

       // Server entrypoint: compiled natively, runs on your machine.
       #[cfg(feature = "server")]
       dioxus::serve(|| async { Ok(dioxus::server::router(App)) });
   }
   ```

   `#[cfg(not(feature = "server"))]` means "only compile this line when the
   `server` feature is *off*" — that's the WASM build, so it calls
   `dioxus::launch` exactly like chapter 1. `#[cfg(feature = "server")]`
   is its mirror image: only compiled into the native server binary. The
   two `dioxus::launch`/`dioxus::serve` calls never coexist in the same
   binary — each build only sees the one that matches its own feature flag.

   `dioxus::serve` takes a closure that returns a `Future` resolving to a
   `Result<Router, _>` — it's async because building the router might need
   to do async setup (connect a database, in later chapters).
   `dioxus::server::router(App)` builds an axum `Router` that serves your
   Dioxus app *and* wires up every `#[server]` function as an HTTP route —
   that wiring is what you're about to use in the next step.

4. **Create `src/server.rs`** — this is where server functions live from now
   on:

   ```rust
   use dioxus::prelude::*;

   #[get("/api/ping")]
   pub async fn ping() -> ServerFnResult<String> {
       Ok("pong from the server 🏓".to_string())
   }
   ```

   A few things are happening in these four lines:

   - `#[get("/api/ping")]` is a Dioxus macro that does two *different*
     things depending on which build it's compiled into. On the server, it
     turns `ping` into a real axum route — `GET /api/ping` — registered by
     `dioxus::server::router` back in `main.rs`. In the WASM build, it
     rewrites the function body into an HTTP client call: `fetch("/api/ping")`,
     decode the JSON response, return it. Either way, callers just write
     `ping().await` and get a `String` back — they can't tell which build
     they're in, and don't need to.
   - `ServerFnResult<String>` is a type alias for
     `Result<String, ServerFnError>` — a normal `Result` whose error type
     knows how to serialize itself across the network (an error on the
     server becomes an error on the client, not a panic or a silent
     failure).
   - The function body only ever *runs* on the server. In the WASM build,
     the macro has already replaced it with the fetch call — the string
     `"pong from the server 🏓"` never ships to the browser as source, only
     as the HTTP response body when someone calls it.

5. **Call it from `App`.** In `app.rs`, hold the result in a signal and
   call `ping()` from a button's `onclick`:

   ```rust
   use crate::server::ping;

   pub fn App() -> Element {
       let mut reply = use_signal(|| Option::<String>::None);

       rsx! {
           style { {CSS} }
           main { class: "wrap",
               h1 { "MyApp" }
               button {
                   onclick: move |_| async move {
                       match ping().await {
                           Ok(msg) => reply.set(Some(msg)),
                           Err(e) => reply.set(Some(format!("error: {e}"))),
                       }
                   },
                   "Ping the server"
               }
               if let Some(msg) = reply() {
                   p { "{msg}" }
               }
           }
       }
   }
   ```

   `onclick` takes a closure; `move |_| async move { ... }` is an async
   closure — Dioxus spawns it as a task when clicked, and `.await`ing
   `ping()` inside it doesn't block the UI. `reply.set(...)` updates the
   signal, which re-renders the `if let Some(msg) = reply()` block
   automatically — no manual DOM manipulation.

Don't worry about getting every detail right — compare your result with
[chapters/02-server-functions](../02-server-functions) once you've had a go.

## Check your work

Open [chapters/02-server-functions](../02-server-functions) and run it with
`dx serve` — that's the answer key for this step.

**Next:** [Chapter 2 — Server functions](../02-server-functions/README.md)

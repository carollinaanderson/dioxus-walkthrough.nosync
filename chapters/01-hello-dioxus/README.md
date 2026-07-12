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
function the browser calls over HTTP. You'll build it up from this chapter's
code a piece at a time. Don't worry about getting every detail right — compare
your result with [chapters/02-server-functions](../02-server-functions) once
you've had a go.

1. **Copy this chapter as your working copy** (don't edit chapters/01 in
   place — keep it as a clean reference):

   ```bash
   cp -r ../01-hello-dioxus ../my-02-server-functions
   cd ../my-02-server-functions
   ```

2. **Turn on Dioxus's `fullstack` feature and add server-only dependencies.**
   Dioxus apps that have a server are compiled *twice*: once to WebAssembly
   for the browser, once to a native binary for the server. Cargo features
   let one `Cargo.toml` describe both builds.

   Start in `Cargo.toml` by switching the `dioxus` line from the `web`
   feature to `fullstack`:

   ```toml
   [dependencies]
   dioxus = { version = "0.7", features = ["fullstack"] } # <-- was ["web"]
   ```

   Add the two server-only dependencies right below it, one line at a time:

   ```toml
   dioxus = { version = "0.7", features = ["fullstack"] }

   # Server-only — not compiled into the WASM bundle.
   axum = { version = "0.8", optional = true }                     # <-- add this
   tokio = { version = "1", features = ["full"], optional = true } # <-- add this
   ```

   `optional = true` is what makes them "server-only": Cargo won't compile
   them unless a feature switches them on. Now add that switch — a new
   `[features]` section at the bottom of the file, one line at a time:

   ```toml
   [features]
   default = ["web"]                                    # <-- web build is the default
   web = ["dioxus/web"]                                 # <-- browser/WASM renderer
   server = ["dioxus/server", "dep:axum", "dep:tokio"]  # <-- native server + its deps
   ```

   Because `default = ["web"]`, `axum` and `tokio` stay out of the WASM
   bundle unless something turns on `server`. `dx serve` builds both feature
   sets for a fullstack app, so you never invoke this split by hand — it does
   it for you.

3. **Branch `main.rs` on which build you're in.** Right now `main.rs` is just:

   ```rust
   mod app;
   use app::App;

   fn main() {
       dioxus::launch(App);
   }
   ```

   First declare the server module you'll create in the next step:

   ```rust
   mod app;
   mod server; // <-- add this
   use app::App;
   ```

   Now replace the single `dioxus::launch(App)` with two `#[cfg]`-gated
   entrypoints. Start with just the client one you already have, now guarded:

   ```rust
   fn main() {
       // Client entrypoint: compiled to WASM, runs in the browser.
       #[cfg(not(feature = "server"))] // <-- add this
       dioxus::launch(App);
   }
   ```

   `#[cfg(not(feature = "server"))]` means "only compile this line when the
   `server` feature is *off*" — the WASM build, unchanged from chapter 1. Add
   its mirror image, the native server entrypoint:

   ```rust
   fn main() {
       // Client entrypoint: compiled to WASM, runs in the browser.
       #[cfg(not(feature = "server"))]
       dioxus::launch(App);

       // Server entrypoint: compiled natively, runs on your machine.
       #[cfg(feature = "server")]                                   // <-- add this
       dioxus::serve(|| async { Ok(dioxus::server::router(App)) }); // <-- add this
   }
   ```

   The two calls never coexist in one binary — each build only sees the arm
   matching its own feature flag. `dioxus::serve` takes a closure returning a
   `Future` that resolves to a `Result<Router, _>` — async because later
   chapters do setup here (connecting a database).
   `dioxus::server::router(App)` builds an axum `Router` that serves your app
   *and* wires up every `#[server]` function as an HTTP route — the wiring
   you're about to use.

4. **Create `src/server.rs`** — where server functions live from now on.
   Start with just the import:

   ```rust
   use dioxus::prelude::*;
   ```

   Add the function as an empty shell first, so its shape is clear before the
   body:

   ```rust
   use dioxus::prelude::*;

   #[get("/api/ping")]
   pub async fn ping() -> ServerFnResult<String> {
       // fill in next
   }
   ```

   Then fill in the one-line body:

   ```rust
   #[get("/api/ping")]
   pub async fn ping() -> ServerFnResult<String> {
       Ok("pong from the server 🏓".to_string()) // <-- add this
   }
   ```

   What those lines do:

   - `#[get("/api/ping")]` is a Dioxus macro that expands *differently* per
     build. On the server it turns `ping` into a real axum route — `GET
     /api/ping` — registered by `dioxus::server::router` back in `main.rs`. In
     the WASM build it rewrites the body into an HTTP client call:
     `fetch("/api/ping")`, decode the JSON response, return it. Either way,
     callers just write `ping().await` and get a `String` back — they can't
     tell which build they're in, and don't need to.
   - `ServerFnResult<String>` is a type alias for
     `Result<String, ServerFnError>` — a normal `Result` whose error type
     serializes across the network, so an error on the server becomes an
     `Err` on the client, not a panic or a silent failure.
   - The body only ever *runs* on the server. In the WASM build the macro has
     already replaced it with the fetch call — the string `"pong from the
     server 🏓"` never ships to the browser as source, only as the HTTP
     response body when someone calls it.

5. **Call it from `App`.** Open `app.rs`. First import the server function,
   just under the existing prelude import:

   ```rust
   use dioxus::prelude::*;
   use crate::server::ping; // <-- add this
   ```

   Add a signal at the top of `App` to hold the reply — `None` until the
   button is clicked:

   ```rust
   pub fn App() -> Element {
       let mut reply = use_signal(|| Option::<String>::None); // <-- add this

       rsx! {
           // ...unchanged for now
       }
   }
   ```

   Now swap chapter 1's static "It's alive!" card for one with a button.
   Add the button with an *empty* `onclick` first:

   ```rust
           section { class: "card",
               h2 { "Say hi to the server" }
               p { "This button calls a `#[server]` function over HTTP and shows what comes back." }
               div { class: "row",
                   button {
                       class: "primary",
                       onclick: move |_| async move {
                           // fill in next
                       },
                       "Ping the server"
                   }
               }
           }
   ```

   `move |_| async move { ... }` is an async closure — Dioxus spawns it as a
   task on click, so `.await`ing inside it doesn't block the UI. Fill it in to
   call `ping` and store the result:

   ```rust
                       onclick: move |_| async move {
                           match ping().await {                                  // <-- add this
                               Ok(msg) => reply.set(Some(msg)),                  // <-- add this
                               Err(e) => reply.set(Some(format!("error: {e}"))), // <-- add this
                           }
                       },
   ```

   Finally, show the reply. Add this right after the `div { class: "row", … }`
   block, still inside the card:

   ```rust
               }
               if let Some(msg) = reply() {     // <-- add this
                   p { class: "mono", "{msg}" } // <-- add this
               }
   ```

   `reply.set(...)` updates the signal; reading `reply()` inside `rsx!`
   subscribes this block to it, so the `if let` re-renders on its own when the
   reply arrives — no manual DOM work. (While you're here, bump the subtitle
   to `p { class: "sub", "Chapter 2: server functions." }` to match.)

## Check your work

Open [chapters/02-server-functions](../02-server-functions) and run it with
`dx serve` — that's the answer key for this step.

**Next:** [Chapter 2 — Server functions](../02-server-functions/README.md)

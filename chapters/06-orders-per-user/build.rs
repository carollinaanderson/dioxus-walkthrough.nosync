//! Load `.env` at build time so compile-time `env!` reads (like
//! `env!("CLERK_PUBLISHABLE_KEY")` in `app.rs`) pick up values from `.env`
//! without having to `export` them into your shell first. `env!` reads the
//! build process's environment, not the `.env` file, so we bridge the two
//! here by re-emitting each entry as `cargo:rustc-env`.
//!
//! A missing `.env` is not an error: the iterator just yields `Err` and the
//! build falls back to whatever is already in the process environment (how
//! CI and the Docker `--build-arg` path supply the key).

fn main() {
    // Rerun this script whenever .env changes (including when it's created).
    println!("cargo:rerun-if-changed=.env");

    if let Ok(iter) = dotenvy::from_path_iter(".env") {
        for (key, value) in iter.flatten() {
            println!("cargo:rustc-env={key}={value}");
        }
    }
}

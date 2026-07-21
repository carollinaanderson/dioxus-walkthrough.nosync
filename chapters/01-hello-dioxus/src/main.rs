mod app;
use app::App;

fn main() {
    hello_world();
    dioxus::launch(App);
    
}

fn hello_world() -> String {
    "Hello, world!".to_string()
}
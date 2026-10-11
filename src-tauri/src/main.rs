// Hide the console window on Windows release builds. Debug builds keep it so
// `println!` diagnostics stay visible during development.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    airprompt_lib::run();
}

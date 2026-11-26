//! Run the native close-sheet focus regression on AppKit's main thread.
fn main() {
    guild_desktop::system::close_prompt::verify_navigation();
    println!("PASS: native close-sheet initial focus and bidirectional Tab cycle");
}

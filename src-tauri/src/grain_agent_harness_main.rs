//! Dedicated debug-only acceptance executable; never an installer target.
fn main() {
    handy_app_lib::run(handy_app_lib::CliArgs::default());
}

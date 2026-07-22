fn main() {
    let args = std::env::args().skip(1);
    match grain_ext_cli::run(args, &std::env::current_dir().unwrap_or_default()) {
        Ok(output) => println!("{output}"),
        Err(error) => {
            eprintln!("grain-ext: {error:#}");
            std::process::exit(1);
        }
    }
}

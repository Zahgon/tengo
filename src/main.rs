fn main() {
    let code = tengo::cli::run(std::env::args().skip(1).collect());
    tengo::out::flush();
    std::process::exit(code);
}

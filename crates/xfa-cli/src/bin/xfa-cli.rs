#[path = "../main.rs"]
mod main_impl;

pub use main_impl::error;
pub use main_impl::parse_page_list;
pub use main_impl::Cli;

fn main() -> anyhow::Result<()> {
    main_impl::main()
}

mod console;
mod excel;
mod html;

pub use console::generate as generate_console;
pub use excel::generate as generate_xlsx;
pub use html::generate as generate_html;

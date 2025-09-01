pub mod confirm;
pub mod output;
pub mod track;

use console::Term;

pub fn width() -> usize {
    let term = Term::stdout();
    let (_, width) = term.size();
    width as usize
}

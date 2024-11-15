use console::style;
use pad::PadStr;

pub struct Table {
    ncol: usize,
    rows: Vec<Vec<TableCell>>,
    foot_index: usize,
}

#[derive(Clone, Copy)]
pub enum TableCellColor {
    Red,
    Green,
    Yellow,
}

pub struct TableCell {
    pub text: String,
    pub color: Option<TableCellColor>,
}

impl TableCell {
    pub fn no_color(text: String) -> Self {
        Self { text, color: None }
    }

    pub fn with_color(text: String, color: TableCellColor) -> Self {
        Self {
            text,
            color: Some(color),
        }
    }
}

impl Table {
    pub fn with_capacity(size: usize) -> Table {
        Table {
            ncol: 0,
            rows: Vec::with_capacity(size),
            foot_index: 0,
        }
    }

    #[inline]
    pub fn foot(&mut self) {
        self.foot_index = self.rows.len();
    }

    pub fn add(&mut self, row: Vec<String>) {
        let row = row.into_iter().map(TableCell::no_color).collect();
        self.add_color(row)
    }

    pub fn add_color(&mut self, row: Vec<TableCell>) {
        if row.is_empty() {
            panic!("empty row");
        }
        if self.ncol == 0 {
            self.ncol = row.len();
        } else if row.len() != self.ncol {
            panic!("unexpected row len");
        }
        self.rows.push(row);
    }

    pub fn show(self) {
        let mut pads = Vec::with_capacity(self.ncol);
        for coli in 0..self.ncol {
            let mut max_size: usize = 0;
            for row in self.rows.iter() {
                let cell = row.get(coli).unwrap();
                let size = console::measure_text_width(&cell.text);
                if size > max_size {
                    max_size = size
                }
            }
            pads.push(max_size);
        }

        let mut split = String::from("+");
        for pad in pads.iter() {
            for _ in 0..*pad + 2 {
                split.push('-');
            }
            split.push('+');
        }

        for (rowi, row) in self.rows.into_iter().enumerate() {
            if rowi == 0 || (self.foot_index > 0 && rowi >= self.foot_index) {
                eprintln!("{split}");
            }
            eprint!("|");
            for (coli, cell) in row.into_iter().enumerate() {
                let pad = pads[coli];
                let mut text = cell
                    .text
                    .pad_to_width_with_alignment(pad, pad::Alignment::Left);
                if let Some(color) = cell.color {
                    let style_text = match color {
                        TableCellColor::Red => style(&text).red(),
                        TableCellColor::Green => style(&text).green(),
                        TableCellColor::Yellow => style(&text).yellow(),
                    };
                    text = format!("{style_text}");
                }
                eprint!(" {text} |");
            }
            eprintln!();

            if rowi == 0 {
                eprintln!("{split}");
            }
        }

        eprintln!("{split}");
    }
}

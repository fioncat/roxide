use std::borrow::Cow;

use pad::PadStr;

/// A utility for rendering data in an ASCII table format.
///
/// The table supports:
/// - Headers (optional)
/// - Dynamic column widths based on content
/// - Unicode character width handling
/// - Automatic padding and alignment
pub struct Table<'a> {
    ncol: usize,
    rows: Vec<Vec<Cow<'a, str>>>,
    headless: bool,
}

impl<'a> Table<'a> {
    /// Creates a new table with the specified initial capacity.
    ///
    /// # Arguments
    /// * `size` - Initial capacity for rows
    /// * `headless` - If true, first row is treated as data. If false, first row is treated as header.
    pub fn with_capacity(size: usize, headless: bool) -> Table<'a> {
        Table {
            ncol: 0,
            rows: Vec::with_capacity(size),
            headless,
        }
    }

    /// Adds a row to the table.
    ///
    /// The first row added determines the number of columns.
    /// Subsequent rows must have the same number of columns.
    ///
    /// # Arguments
    /// * `row` - Vector of strings representing the row data
    ///
    /// # Panics
    /// Panics if a row has a different number of columns than the first row
    pub fn add(&mut self, row: Vec<Cow<'a, str>>) {
        if self.ncol == 0 {
            self.ncol = row.len();
            if self.headless {
                return;
            }
        } else if row.len() != self.ncol {
            panic!("unexpected row len");
        }
        self.rows.push(row);
    }

    pub fn add_static(&mut self, row: Vec<&'a str>) {
        let cow_row: Vec<Cow<'a, str>> = row.into_iter().map(Cow::from).collect();
        self.add(cow_row);
    }

    /// Renders the table to a string.
    ///
    /// # Returns
    /// Returns a string containing the rendered ASCII table
    pub fn render(self) -> String {
        let mut pads = Vec::with_capacity(self.ncol);
        for coli in 0..self.ncol {
            let mut max_size: usize = 0;
            for row in self.rows.iter() {
                let cell = row.get(coli).unwrap();
                let size = console::measure_text_width(cell);
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

        let mut result = String::new();
        for (rowi, row) in self.rows.into_iter().enumerate() {
            if rowi == 0 {
                result.push_str(&split);
                result.push('\n');
            }
            result.push('|');
            for (coli, cell) in row.into_iter().enumerate() {
                let pad = pads[coli];
                let text = cell.pad_to_width_with_alignment(pad, pad::Alignment::Left);

                result.push(' ');
                result.push_str(&text);
                result.push_str(" |");
            }
            result.push('\n');

            if !self.headless && rowi == 0 {
                result.push_str(&split);
                result.push('\n');
            }
        }

        result.push_str(&split);
        result.push('\n');

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_table() {
        struct Case {
            rows: Vec<Vec<&'static str>>,
            headless: bool,
            expect: &'static str,
        }
        let cases = [
            Case {
                rows: vec![vec!["Name", "Age"], vec!["Alice", "20"]],
                headless: false,
                expect: "+-------+-----+\n\
                         | Name  | Age |\n\
                         +-------+-----+\n\
                         | Alice | 20  |\n\
                         +-------+-----+\n",
            },
            Case {
                rows: vec![vec!["名字", "年龄"], vec!["张三", "20"]],
                headless: false,
                expect: "+------+------+\n\
                         | 名字 | 年龄 |\n\
                         +------+------+\n\
                         | 张三 | 20   |\n\
                         +------+------+\n",
            },
            Case {
                rows: vec![vec!["Data1", "Data2"], vec!["Value1", "Value2"]],
                headless: true,
                expect: "+--------+--------+\n\
                         | Value1 | Value2 |\n\
                         +--------+--------+\n",
            },
            Case {
                rows: vec![
                    vec!["Name", "Age", "City"],
                    vec!["Alice", "20", "New York"],
                    vec!["Bob", "25", "London"],
                    vec!["Carol", "30", "Tokyo"],
                    vec!["David", "35", "Paris"],
                ],
                headless: false,
                expect: "+-------+-----+----------+\n\
                         | Name  | Age | City     |\n\
                         +-------+-----+----------+\n\
                         | Alice | 20  | New York |\n\
                         | Bob   | 25  | London   |\n\
                         | Carol | 30  | Tokyo    |\n\
                         | David | 35  | Paris    |\n\
                         +-------+-----+----------+\n",
            },
        ];

        for case in cases {
            let mut table = Table::with_capacity(case.rows.len(), case.headless);
            for row in case.rows {
                table.add_static(row);
            }
            let result = table.render();
            assert_eq!(result, case.expect);
        }
    }

    #[test]
    #[should_panic(expected = "unexpected row len")]
    fn test_invalid_column_count() {
        let mut table = Table::with_capacity(2, false);
        table.add_static(vec!["Col1", "Col2"]);
        table.add_static(vec!["Value1"]); // Should panic
    }
}

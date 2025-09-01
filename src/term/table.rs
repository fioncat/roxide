use pad::PadStr;

/// A utility for rendering data in an ASCII table format.
///
/// The table supports:
/// - Headers (optional)
/// - Dynamic column widths based on content
/// - Unicode character width handling
/// - Automatic padding and alignment
pub struct Table {
    ncol: usize,
    rows: Vec<Vec<String>>,
    headless: bool,
}

impl Table {
    /// Creates a new table with the specified initial capacity.
    ///
    /// # Arguments
    /// * `size` - Initial capacity for rows
    /// * `headless` - If true, first row is treated as data. If false, first row is treated as header.
    pub fn with_capacity(size: usize, headless: bool) -> Table {
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
    pub fn add(&mut self, row: Vec<String>) {
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
        let test_cases = [
            // Test basic table
            (
                vec![
                    vec!["Name".to_string(), "Age".to_string()],
                    vec!["Alice".to_string(), "20".to_string()],
                ],
                false,
                "+-------+-----+\n\
                 | Name  | Age |\n\
                 +-------+-----+\n\
                 | Alice | 20  |\n\
                 +-------+-----+\n",
            ),
            // Test unicode width
            (
                vec![
                    vec!["名字".to_string(), "年龄".to_string()],
                    vec!["张三".to_string(), "20".to_string()],
                ],
                false,
                "+------+------+\n\
                 | 名字 | 年龄 |\n\
                 +------+------+\n\
                 | 张三 | 20   |\n\
                 +------+------+\n",
            ),
            // Test headless table
            (
                vec![
                    vec!["Data1".to_string(), "Data2".to_string()],
                    vec!["Value1".to_string(), "Value2".to_string()],
                ],
                true,
                "+--------+--------+\n\
                 | Value1 | Value2 |\n\
                 +--------+--------+\n",
            ),
            // Test multi-line table
            (
                vec![
                    vec![
                        "Name".to_string(),
                        "Age".to_string(),
                        "City".to_string(),
                    ],
                    vec![
                        "Alice".to_string(),
                        "20".to_string(),
                        "New York".to_string(),
                    ],
                    vec![
                        "Bob".to_string(),
                        "25".to_string(),
                        "London".to_string(),
                    ],
                    vec![
                        "Carol".to_string(),
                        "30".to_string(),
                        "Tokyo".to_string(),
                    ],
                    vec![
                        "David".to_string(),
                        "35".to_string(),
                        "Paris".to_string(),
                    ],
                ],
                false,
                "+-------+-----+----------+\n\
                 | Name  | Age | City     |\n\
                 +-------+-----+----------+\n\
                 | Alice | 20  | New York |\n\
                 | Bob   | 25  | London   |\n\
                 | Carol | 30  | Tokyo    |\n\
                 | David | 35  | Paris    |\n\
                 +-------+-----+----------+\n",
            ),
        ];

        for (rows, headless, expected) in test_cases {
            let mut table = Table::with_capacity(rows.len(), headless);
            for row in rows {
                table.add(row);
            }
            let result = table.render();
            assert_eq!(result, expected);
        }
    }

    #[test]
    #[should_panic(expected = "unexpected row len")]
    fn test_invalid_column_count() {
        let mut table = Table::with_capacity(2, false);
        table.add(vec!["Col1".to_string(), "Col2".to_string()]);
        table.add(vec!["Value1".to_string()]); // Should panic
    }
}

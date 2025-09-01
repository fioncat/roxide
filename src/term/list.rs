use anyhow::Result;
use serde::{Serialize, de::DeserializeOwned};

use crate::term::table::Table;

pub trait ListItem {
    fn titles() -> Vec<&'static str>;
    fn row(self) -> Vec<String>;
}

pub fn render_list<T>(
    list: Vec<T>,
    page: usize,
    page_size: usize,
    total: usize,
    json: bool,
    headless: bool,
) -> Result<String>
where
    T: Serialize + DeserializeOwned + ListItem,
{
    if json {
        let json = serde_json::to_string_pretty(&list)?;
        return Ok(json);
    }
    if list.is_empty() {
        return Ok(String::from("<empty list>"));
    }

    let mut table = Table::with_capacity(list.len(), headless);
    let titles = T::titles();
    table.add(titles.iter().map(|s| s.to_string()).collect());

    for item in list {
        let row = item.row();
        table.add(row);
    }

    let total_pages = total.div_ceil(page_size);
    let hint = format!("Page: {page}/{total_pages}, Total: {total}");
    Ok(format!("{hint}\n{}", table.render()))
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    use super::*;

    #[derive(Serialize, Deserialize)]
    struct User {
        id: u32,
        name: String,
        email: String,
    }

    impl ListItem for User {
        fn titles() -> Vec<&'static str> {
            vec!["ID", "Name", "Email"]
        }

        fn row(self) -> Vec<String> {
            vec![self.id.to_string(), self.name, self.email]
        }
    }

    #[test]
    fn test_render() {
        let test_cases = [
            (vec![], 1, 5, 0, false, "<empty list>"),
            (
                vec![
                    User {
                        id: 1,
                        name: "Alice".to_string(),
                        email: "test1@123.com".to_string(),
                    },
                    User {
                        id: 2,
                        name: "Bob".to_string(),
                        email: "test2@123.com".to_string(),
                    },
                    User {
                        id: 3,
                        name: "Charlie".to_string(),
                        email: "test3@33.com".to_string(),
                    },
                ],
                2,
                3,
                30,
                false,
                "Page: 2/10, Total: 30\n\
                 +----+---------+---------------+\n\
                 | ID | Name    | Email         |\n\
                 +----+---------+---------------+\n\
                 | 1  | Alice   | test1@123.com |\n\
                 | 2  | Bob     | test2@123.com |\n\
                 | 3  | Charlie | test3@33.com  |\n\
                 +----+---------+---------------+\n",
            ),
            (
                vec![
                    User {
                        id: 1,
                        name: "Alice".to_string(),
                        email: "alice@example.com".to_string(),
                    },
                    User {
                        id: 2,
                        name: "Bob".to_string(),
                        email: "bob@example.com".to_string(),
                    },
                ],
                1,
                10,
                2,
                true,
                "Page: 1/1, Total: 2\n\
                 +---+-------+-------------------+\n\
                 | 1 | Alice | alice@example.com |\n\
                 | 2 | Bob   | bob@example.com   |\n\
                 +---+-------+-------------------+\n",
            ),
        ];

        for (list, page, page_size, total, headless, expected) in test_cases {
            let result = render_list(list, page, page_size, total, false, headless).unwrap();
            assert_eq!(result, expected);
        }
    }

    #[test]
    fn test_json() {
        let users = vec![
            User {
                id: 1,
                name: "Alice".to_string(),
                email: "alice@example.com".to_string(),
            },
            User {
                id: 2,
                name: "Bob".to_string(),
                email: "bob@example.com".to_string(),
            },
        ];
        let expected = serde_json::to_string_pretty(&users).unwrap();
        let result = render_list(users, 1, 10, 2, true, false).unwrap();
        assert_eq!(result, expected);
    }
}

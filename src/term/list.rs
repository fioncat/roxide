use std::borrow::Cow;

use anyhow::Result;
use clap::Args;
use serde::Serialize;

use crate::db::repo::LimitOptions;
use crate::term::table::Table;

pub trait List<T: ListItem> {
    fn titles(&self) -> Vec<&'static str>;

    fn total(&self) -> u32;

    fn items(&self) -> &[T];
}

pub trait ListItem: Serialize {
    fn row<'a>(&'a self, title: &str) -> Cow<'a, str>;
}

#[derive(Debug, Clone, Copy, Args)]
pub struct ListArgs {
    #[arg(long, short)]
    pub page: u32,

    #[arg(long, short = 's', default_value = "20")]
    pub page_size: u32,
}

impl ListArgs {
    fn limit_options(&self) -> LimitOptions {
        let offset = (self.page - 1) * self.page_size;
        let limit = self.page_size;
        LimitOptions { offset, limit }
    }
}

pub fn render_list<L, T>(
    list: L,
    json: bool,
    headless: bool,
    args: Option<ListArgs>,
) -> Result<String>
where
    L: List<T>,
    T: ListItem,
{
    let items = list.items();
    if json {
        let json = serde_json::to_string_pretty(items)?;
        return Ok(json);
    }
    if items.is_empty() {
        return Ok(String::from("<empty list>"));
    }

    let titles = list.titles();
    let total = list.total();

    let mut table = Table::with_capacity(items.len(), headless);
    table.add_static(titles.clone());

    for item in items {
        let mut row = Vec::with_capacity(titles.len());
        for title in titles.iter() {
            let cell = item.row(title);
            row.push(cell);
        }
        table.add(row);
    }

    if let Some(args) = args {
        let total_pages = total.div_ceil(args.page_size);
        let hint = format!("Page: {}/{total_pages}, Total: {}", args.page, total);
        return Ok(format!("{hint}\n{}", table.render()));
    }

    Ok(table.render())
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
        fn row<'a>(&'a self, title: &str) -> Cow<'a, str> {
            match title {
                "ID" => self.id.to_string().into(),
                "Name" => Cow::Borrowed(&self.name),
                "Email" => Cow::Borrowed(&self.email),
                _ => Cow::Borrowed(""),
            }
        }
    }

    struct UserList {
        users: Vec<User>,
        total: u32,
    }

    impl List<User> for UserList {
        fn titles(&self) -> Vec<&'static str> {
            vec!["ID", "Name", "Email"]
        }

        fn total(&self) -> u32 {
            self.total
        }

        fn items(&self) -> &[User] {
            self.users.as_ref()
        }
    }

    #[test]
    fn test_render() {
        struct Case {
            users: Vec<User>,
            headless: bool,
            total: u32,
            args: Option<ListArgs>,
            expect: &'static str,
        }

        let cases = [
            Case {
                users: vec![],
                headless: false,
                total: 0,
                args: None,
                expect: "<empty list>",
            },
            Case {
                users: vec![
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
                headless: false,
                total: 30,
                args: Some(ListArgs {
                    page: 2,
                    page_size: 3,
                }),
                expect: "Page: 2/10, Total: 30\n\
                         +----+---------+---------------+\n\
                         | ID | Name    | Email         |\n\
                         +----+---------+---------------+\n\
                         | 1  | Alice   | test1@123.com |\n\
                         | 2  | Bob     | test2@123.com |\n\
                         | 3  | Charlie | test3@33.com  |\n\
                         +----+---------+---------------+\n",
            },
            Case {
                users: vec![
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
                headless: true,
                total: 2,
                args: Some(ListArgs {
                    page: 1,
                    page_size: 2,
                }),
                expect: "Page: 1/1, Total: 2\n\
                         +---+-------+-------------------+\n\
                         | 1 | Alice | alice@example.com |\n\
                         | 2 | Bob   | bob@example.com   |\n\
                         +---+-------+-------------------+\n",
            },
        ];

        for case in cases {
            let list = UserList {
                users: case.users,
                total: case.total,
            };
            let result = render_list(list, false, case.headless, case.args).unwrap();
            assert_eq!(result, case.expect);
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
        let total = users.len() as u32;
        let list = UserList { users, total };
        let result = render_list(list, true, false, None).unwrap();
        assert_eq!(result, expected);
    }
}

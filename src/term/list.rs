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
    #[arg(long)]
    pub json: bool,

    #[arg(long)]
    pub headless: bool,

    #[arg(long, short, default_value = "1")]
    pub page: u32,

    #[arg(long, short = 's', default_value = "20")]
    pub page_size: u32,
}

impl ListArgs {
    pub fn limit(&self) -> LimitOptions {
        let offset = (self.page - 1) * self.page_size;
        let limit = self.page_size;
        LimitOptions { offset, limit }
    }

    pub fn render<L, T>(&self, list: L) -> Result<String>
    where
        L: List<T>,
        T: ListItem,
    {
        let items = list.items();
        if self.json {
            let json = serde_json::to_string_pretty(items)?;
            return Ok(json);
        }
        if items.is_empty() {
            return Ok(String::from("<empty list>"));
        }

        let titles = list.titles();
        let total = list.total();

        let mut table = Table::with_capacity(items.len(), self.headless);
        table.add_static(titles.clone());

        for item in items {
            let mut row = Vec::with_capacity(titles.len());
            for title in titles.iter() {
                let cell = item.row(title);
                row.push(cell);
            }
            table.add(row);
        }

        let total_pages = total.div_ceil(self.page_size);
        let hint = format!("Page: {}/{total_pages}, Total: {}", self.page, total);
        Ok(format!("{hint}\n{}", table.render()))
    }
}

pub fn pagination<T>(list: Vec<T>, opts: LimitOptions) -> (Vec<T>, u32) {
    let total = list.len() as u32;

    let start = opts.offset as usize;
    let end = (start + opts.limit as usize).min(list.len());

    if start >= list.len() {
        return (Vec::new(), total);
    }

    let paginated_items = list.into_iter().skip(start).take(end - start).collect();

    (paginated_items, total)
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    use super::*;

    #[derive(Clone, Serialize, Deserialize)]
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
            total: u32,
            args: ListArgs,
            expect: &'static str,
        }

        let cases = [
            Case {
                users: vec![],
                total: 0,
                args: ListArgs {
                    headless: false,
                    page: 1,
                    page_size: 10,
                    json: false,
                },
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
                total: 30,
                args: ListArgs {
                    page: 2,
                    page_size: 3,
                    headless: false,
                    json: false,
                },
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
                total: 2,
                args: ListArgs {
                    page: 1,
                    page_size: 2,
                    headless: true,
                    json: false,
                },
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
            let result = case.args.render(list).unwrap();
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
        let args = ListArgs {
            page: 1,
            page_size: 20,
            json: true,
            headless: false,
        };
        let result = args.render(list).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_pagination() {
        struct Case {
            users: Vec<User>,
            limit_options: LimitOptions,
            expect_total: u32,
            expect_count: usize,
            expect_ids: Vec<u32>,
        }

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
            User {
                id: 3,
                name: "Charlie".to_string(),
                email: "charlie@example.com".to_string(),
            },
            User {
                id: 4,
                name: "David".to_string(),
                email: "david@example.com".to_string(),
            },
            User {
                id: 5,
                name: "Eve".to_string(),
                email: "eve@example.com".to_string(),
            },
        ];

        let cases = [
            Case {
                users: users.clone(),
                limit_options: LimitOptions {
                    offset: 0,
                    limit: 2,
                },
                expect_total: 5,
                expect_count: 2,
                expect_ids: vec![1, 2],
            },
            Case {
                users: users.clone(),
                limit_options: LimitOptions {
                    offset: 2,
                    limit: 2,
                },
                expect_total: 5,
                expect_count: 2,
                expect_ids: vec![3, 4],
            },
            Case {
                users: users.clone(),
                limit_options: LimitOptions {
                    offset: 4,
                    limit: 2,
                },
                expect_total: 5,
                expect_count: 1,
                expect_ids: vec![5],
            },
            Case {
                users: users.clone(),
                limit_options: LimitOptions {
                    offset: 6,
                    limit: 2,
                },
                expect_total: 5,
                expect_count: 0,
                expect_ids: vec![],
            },
            Case {
                users: users.clone(),
                limit_options: LimitOptions {
                    offset: 0,
                    limit: 3,
                },
                expect_total: 5,
                expect_count: 3,
                expect_ids: vec![1, 2, 3],
            },
            Case {
                users: users.clone(),
                limit_options: LimitOptions {
                    offset: 3,
                    limit: 3,
                },
                expect_total: 5,
                expect_count: 2,
                expect_ids: vec![4, 5],
            },
            Case {
                users: vec![],
                limit_options: LimitOptions {
                    offset: 0,
                    limit: 10,
                },
                expect_total: 0,
                expect_count: 0,
                expect_ids: vec![],
            },
        ];

        for case in cases {
            let (result, total) = pagination(case.users, case.limit_options);
            assert_eq!(total, case.expect_total);
            assert_eq!(result.len(), case.expect_count);

            for (i, expected_id) in case.expect_ids.iter().enumerate() {
                assert_eq!(result[i].id, *expected_id);
            }
        }
    }
}

pub mod attach;
pub mod branch;
pub mod home;
pub mod owner;
pub mod release;
pub mod remote;
pub mod tag;

pub struct Complete {
    pub items: Vec<String>,

    pub no_space: bool,
}

impl From<Vec<String>> for Complete {
    fn from(items: Vec<String>) -> Self {
        Complete {
            items,
            no_space: false,
        }
    }
}

impl Complete {
    pub fn empty() -> Complete {
        Complete {
            items: vec![],
            no_space: false,
        }
    }

    pub fn no_space(mut self) -> Self {
        self.no_space = true;
        self
    }

    pub fn show(&self) {
        if self.no_space {
            println!("1");
        } else {
            println!("0");
        }
        for item in self.items.iter() {
            println!("{}", item);
        }
    }
}

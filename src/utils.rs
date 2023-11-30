#[macro_export]
macro_rules! vec_strings {
    ( $( $s:expr ),* ) => {
        vec![
            $(
                String::from($s),
            )*
        ]
    };
}

#[macro_export]
macro_rules! hash_set {
    ( $( $x:expr ),* ) => {
        {
            let mut set = HashSet::new();
            $(
                set.insert($x);
            )*
            set
        }
    };
}

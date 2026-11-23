mod p0;

pub use crate::model::Check;

macro_rules! check {
    ($id:literal, $run:ident) => {
        $crate::model::Check {
            id: $id,
            name: stringify!($run),
            run: $run,
        }
    };
}

pub(crate) use check;

pub fn all_checks() -> Vec<Check> {
    let mut checks = Vec::new();
    p0::register(&mut checks);
    checks
}

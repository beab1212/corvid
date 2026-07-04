//! Built-in scalar functions callable from transforms and filter expressions.
//!
//! Each builtin is a pure function over a small argument vector. They are
//! looked up by name at compile time and by id at run time, so adding one here
//! makes it available to both the VM assembler and the filter evaluator.

pub type Args<'a> = &'a [i64];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Builtin {
    pub id: u16,
    pub name: &'static str,
    pub arity: u8,
}

macro_rules! builtins {
    ($($id:expr => $name:literal / $arity:expr),+ $(,)?) => {
        pub const BUILTINS: &[Builtin] = &[
            $(Builtin { id: $id, name: $name, arity: $arity }),+
        ];
    };
}

builtins! {
    0 => "min" / 2,
    1 => "max" / 2,
    2 => "clamp" / 3,
    3 => "abs" / 1,
    4 => "sign" / 1,
    5 => "popcount" / 1,
    6 => "leading_zeros" / 1,
    7 => "gcd" / 2,
    8 => "ceil_div" / 2,
    9 => "wrap_add" / 2,
    10 => "select" / 3,
}

pub fn lookup(name: &str) -> Option<Builtin> {
    BUILTINS.iter().copied().find(|b| b.name == name)
}

pub fn by_id(id: u16) -> Option<Builtin> {
    BUILTINS.iter().copied().find(|b| b.id == id)
}

/// Evaluate builtin `id` over `args`. Returns `None` on arity mismatch.
pub fn eval(id: u16, args: Args) -> Option<i64> {
    let b = by_id(id)?;
    if args.len() != b.arity as usize {
        return None;
    }
    Some(match id {
        0 => args[0].min(args[1]),
        1 => args[0].max(args[1]),
        2 => args[0].clamp(args[1], args[2]),
        3 => args[0].wrapping_abs(),
        4 => args[0].signum(),
        5 => args[0].count_ones() as i64,
        6 => args[0].leading_zeros() as i64,
        7 => gcd(args[0], args[1]),
        8 => ceil_div(args[0], args[1]),
        9 => args[0].wrapping_add(args[1]),
        10 => {
            if args[0] != 0 {
                args[1]
            } else {
                args[2]
            }
        }
        _ => return None,
    })
}

fn gcd(mut a: i64, mut b: i64) -> i64 {
    a = a.wrapping_abs();
    b = b.wrapping_abs();
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
}

fn ceil_div(a: i64, b: i64) -> i64 {
    if b == 0 {
        return 0;
    }
    (a + b - 1) / b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_and_eval() {
        let b = lookup("clamp").unwrap();
        assert_eq!(eval(b.id, &[15, 0, 10]), Some(10));
        assert_eq!(eval(lookup("gcd").unwrap().id, &[12, 8]), Some(4));
    }

    #[test]
    fn arity_mismatch_is_none() {
        assert_eq!(eval(lookup("min").unwrap().id, &[1]), None);
    }

    #[test]
    fn every_builtin_has_unique_id() {
        for (i, b) in BUILTINS.iter().enumerate() {
            assert_eq!(b.id as usize, i);
        }
    }
}

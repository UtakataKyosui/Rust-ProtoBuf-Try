fn main() {
    println!("{}", banner());
}

fn banner() -> &'static str {
    "protolive-server"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn banner_is_stable() {
        assert_eq!(banner(), "protolive-server");
    }
}

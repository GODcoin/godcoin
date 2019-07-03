pub fn parse_line(line: &str) -> Vec<String> {
    let mut vec = Vec::with_capacity(32);
    let mut tmp_str = String::with_capacity(line.len());
    let mut in_quotes = false;

    for c in line.chars() {
        match c {
            ' ' => {
                if in_quotes {
                    tmp_str.push(' ');
                    continue;
                }
                if !tmp_str.is_empty() {
                    vec.push(tmp_str);
                }
                tmp_str = String::with_capacity(line.len());
            }
            '"' => {
                in_quotes = !in_quotes;
            }
            _ => {
                tmp_str.push(c);
            }
        }
    }

    if !tmp_str.is_empty() {
        vec.push(tmp_str);
    }
    vec
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_single_arg() {
        let args = parse_line("abc");
        assert_eq!(args.len(), 1);
        assert_eq!(args[0], "abc");

        let args = parse_line("abc ");
        assert_eq!(args.len(), 1);
        assert_eq!(args[0], "abc");

        let args = parse_line("abc   ");
        assert_eq!(args.len(), 1);
        assert_eq!(args[0], "abc");
    }

    #[test]
    fn parse_two_args() {
        let args = parse_line("abc 123");
        assert_eq!(args.len(), 2);
        assert_eq!(args[0], "abc");
        assert_eq!(args[1], "123");

        let args = parse_line("abc  123");
        assert_eq!(args.len(), 2);
        assert_eq!(args[0], "abc");
        assert_eq!(args[1], "123");
    }

    #[test]
    fn parse_multiargs() {
        let args = parse_line("abc  123    def");
        assert_eq!(args.len(), 3);
        assert_eq!(args[0], "abc");
        assert_eq!(args[1], "123");
        assert_eq!(args[2], "def");

        let args = parse_line("abc 123    def 456");
        assert_eq!(args.len(), 4);
        assert_eq!(args[0], "abc");
        assert_eq!(args[1], "123");
        assert_eq!(args[2], "def");
        assert_eq!(args[3], "456");
    }

    #[test]
    fn parse_quotes() {
        let args = parse_line("\"abc  123\"    def");
        assert_eq!(args.len(), 2);
        assert_eq!(args[0], "abc  123");
        assert_eq!(args[1], "def");

        let args = parse_line("abc\"  \"123    def");
        assert_eq!(args.len(), 2);
        assert_eq!(args[0], "abc  123");
        assert_eq!(args[1], "def");

        let args = parse_line("\"  \"123 def");
        assert_eq!(args.len(), 2);
        assert_eq!(args[0], "  123");
        assert_eq!(args[1], "def");

        let args = parse_line("\"  abc");
        assert_eq!(args.len(), 1);
        assert_eq!(args[0], "  abc");
    }
}

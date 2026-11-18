use std::iter::Peekable;
use std::str::Chars;

#[derive(Debug, PartialEq, Clone)]
enum Token {
    Number(f64),
    Plus,
    Minus,
    Multiply,
    Divide,
    Modulo,
    Power,
    LParen,
    RParen,
    Comma,
    Ident(String),
}

struct Lexer<'a> {
    chars: Peekable<Chars<'a>>,
}

impl<'a> Lexer<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            chars: input.chars().peekable(),
        }
    }

    fn tokenize(&mut self) -> Result<Vec<Token>, String> {
        let mut tokens = Vec::new();
        while let Some(&ch) = self.chars.peek() {
            match ch {
                ' ' | '\t' | '\r' | '\n' => {
                    self.chars.next();
                }
                '+' => {
                    self.chars.next();
                    tokens.push(Token::Plus);
                }
                '-' => {
                    self.chars.next();
                    tokens.push(Token::Minus);
                }
                '*' => {
                    self.chars.next();
                    tokens.push(Token::Multiply);
                }
                '/' => {
                    self.chars.next();
                    tokens.push(Token::Divide);
                }
                '%' => {
                    self.chars.next();
                    // Check if followed by "of" (e.g. 15% of 2400)
                    tokens.push(Token::Modulo);
                }
                '^' => {
                    self.chars.next();
                    tokens.push(Token::Power);
                }
                '(' => {
                    self.chars.next();
                    tokens.push(Token::LParen);
                }
                ')' => {
                    self.chars.next();
                    tokens.push(Token::RParen);
                }
                ',' => {
                    self.chars.next();
                    tokens.push(Token::Comma);
                }
                '0'..='9' | '.' => {
                    let num = self.read_number()?;
                    tokens.push(Token::Number(num));
                }
                'a'..='z' | 'A'..='Z' | '_' => {
                    let ident = self.read_ident();
                    if ident.eq_ignore_ascii_case("of") {
                        // "of" after a percentage functions as multiplication
                        tokens.push(Token::Multiply);
                    } else {
                        tokens.push(Token::Ident(ident));
                    }
                }
                _ => {
                    return Err(format!("Unexpected character: '{ch}'"));
                }
            }
        }
        Ok(tokens)
    }

    fn read_number(&mut self) -> Result<f64, String> {
        let mut s = String::new();
        let mut has_dot = false;
        while let Some(&ch) = self.chars.peek() {
            if ch.is_ascii_digit() {
                s.push(ch);
                self.chars.next();
            } else if ch == '.' && !has_dot {
                has_dot = true;
                s.push(ch);
                self.chars.next();
            } else {
                break;
            }
        }
        let mut val = s.parse::<f64>()
            .map_err(|e| format!("Invalid number '{s}': {e}"))?;
        if let Some(&'%') = self.chars.peek() {
            self.chars.next();
            val /= 100.0;
        }
        Ok(val)
    }

    fn read_ident(&mut self) -> String {
        let mut s = String::new();
        while let Some(&ch) = self.chars.peek() {
            if ch.is_alphanumeric() || ch == '_' {
                s.push(ch);
                self.chars.next();
            } else {
                break;
            }
        }
        s
    }
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn next(&mut self) -> Option<&Token> {
        if self.pos < self.tokens.len() {
            let tok = &self.tokens[self.pos];
            self.pos += 1;
            Some(tok)
        } else {
            None
        }
    }

    pub fn parse(&mut self) -> Result<f64, String> {
        let val = self.expr()?;
        if let Some(tok) = self.peek() {
            return Err(format!("Unexpected token remaining: {tok:?}"));
        }
        Ok(val)
    }

    // expr = term (( '+' | '-' ) term)*
    fn expr(&mut self) -> Result<f64, String> {
        let mut left = self.term()?;
        while let Some(tok) = self.peek() {
            match tok {
                Token::Plus => {
                    self.next();
                    let right = self.term()?;
                    left += right;
                }
                Token::Minus => {
                    self.next();
                    let right = self.term()?;
                    left -= right;
                }
                _ => break,
            }
        }
        Ok(left)
    }

    // term = factor (( '*' | '/' | '%' ) factor)*
    fn term(&mut self) -> Result<f64, String> {
        let mut left = self.power()?;
        while let Some(tok) = self.peek() {
            match tok {
                Token::Multiply => {
                    self.next();
                    let right = self.power()?;
                    left *= right;
                }
                Token::Divide => {
                    self.next();
                    let right = self.power()?;
                    if right == 0.0 {
                        return Err("Division by zero".to_string());
                    }
                    left /= right;
                }
                Token::Modulo => {
                    self.next();
                    let right = self.power()?;
                    if right == 0.0 {
                        return Err("Modulo by zero".to_string());
                    }
                    left %= right;
                }
                _ => break,
            }
        }
        Ok(left)
    }

    // power = unary ( '^' power )?
    fn power(&mut self) -> Result<f64, String> {
        let left = self.unary()?;
        if let Some(Token::Power) = self.peek() {
            self.next();
            let right = self.power()?;
            Ok(left.powf(right))
        } else {
            Ok(left)
        }
    }

    // unary = ( '+' | '-' )? primary
    fn unary(&mut self) -> Result<f64, String> {
        if let Some(Token::Plus) = self.peek() {
            self.next();
            self.unary()
        } else if let Some(Token::Minus) = self.peek() {
            self.next();
            let val = self.unary()?;
            Ok(-val)
        } else {
            self.primary()
        }
    }

    // primary = Number | Ident '(' args ')' | '(' expr ')'
    fn primary(&mut self) -> Result<f64, String> {
        match self.next() {
            Some(Token::Number(n)) => Ok(*n),
            Some(Token::Ident(name)) => {
                let func_name = name.clone();
                if let Some(Token::LParen) = self.peek() {
                    self.next(); // consume '('
                    let mut args = Vec::new();
                    if let Some(Token::RParen) = self.peek() {
                        self.next(); // consume ')'
                    } else {
                        loop {
                            let arg = self.expr()?;
                            args.push(arg);
                            match self.peek() {
                                Some(Token::Comma) => {
                                    self.next();
                                }
                                Some(Token::RParen) => {
                                    self.next();
                                    break;
                                }
                                _ => return Err("Expected ',' or ')' in function arguments".to_string()),
                            }
                        }
                    }
                    self.eval_function(&func_name, &args)
                } else {
                    match func_name.to_ascii_lowercase().as_str() {
                        "pi" => Ok(std::f64::consts::PI),
                        "e" => Ok(std::f64::consts::E),
                        _ => Err(format!("Unknown variable or function '{func_name}'")),
                    }
                }
            }
            Some(Token::LParen) => {
                let val = self.expr()?;
                if let Some(Token::RParen) = self.next() {
                    Ok(val)
                } else {
                    Err("Missing closing parenthesis ')'".to_string())
                }
            }
            Some(other) => Err(format!("Unexpected token: {other:?}")),
            None => Err("Unexpected end of expression".to_string()),
        }
    }

    fn eval_function(&self, name: &str, args: &[f64]) -> Result<f64, String> {
        match name.to_ascii_lowercase().as_str() {
            "abs" => {
                if args.len() != 1 {
                    return Err("abs() expects 1 argument".to_string());
                }
                Ok(args[0].abs())
            }
            "sqrt" => {
                if args.len() != 1 {
                    return Err("sqrt() expects 1 argument".to_string());
                }
                if args[0] < 0.0 {
                    return Err("sqrt() of negative number".to_string());
                }
                Ok(args[0].sqrt())
            }
            "round" => {
                if args.len() == 1 {
                    Ok(args[0].round())
                } else if args.len() == 2 {
                    let factor = 10f64.powi(args[1] as i32);
                    Ok((args[0] * factor).round() / factor)
                } else {
                    Err("round() expects 1 or 2 arguments".to_string())
                }
            }
            "min" => {
                if args.is_empty() {
                    return Err("min() expects at least 1 argument".to_string());
                }
                Ok(args.iter().cloned().fold(f64::INFINITY, f64::min))
            }
            "max" => {
                if args.is_empty() {
                    return Err("max() expects at least 1 argument".to_string());
                }
                Ok(args.iter().cloned().fold(f64::NEG_INFINITY, f64::max))
            }
            "sum" => Ok(args.iter().sum()),
            "avg" | "mean" => {
                if args.is_empty() {
                    return Err("avg() expects at least 1 argument".to_string());
                }
                Ok(args.iter().sum::<f64>() / args.len() as f64)
            }
            _ => Err(format!("Unknown function '{name}'")),
        }
    }
}

/// Evaluates a mathematical expression deterministically.
pub fn eval_math(expr: &str) -> Result<f64, String> {
    let mut lexer = Lexer::new(expr);
    let tokens = lexer.tokenize()?;
    if tokens.is_empty() {
        return Err("Empty expression".to_string());
    }
    let mut parser = Parser::new(tokens);
    parser.parse()
}

/// Formats calculated float cleanly (e.g. integer formatted without trailing .0, else up to 4 decimal places).
pub fn format_result(val: f64) -> String {
    if val.fract() == 0.0 && val.abs() < 1e15 {
        format!("{:.0}", val)
    } else {
        // Strip trailing zeros after decimal point
        let s = format!("{:.4}", val);
        let trimmed = s.trim_end_matches('0');
        trimmed.trim_end_matches('.').to_string()
    }
}

/// Extracts a calculation request from a query if present.
pub fn find_math_intent(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    let prefixes = [
        "calculate ",
        "compute ",
        "evaluate ",
        "what is ",
        "calc ",
    ];

    for p in prefixes {
        if let Some(pos) = lower.find(p) {
            let candidate = &text[pos + p.len()..];
            let cleaned = candidate.trim_end_matches('?').trim_end_matches('.').trim();
            // Validate if it contains mathematical characters or keywords
            if cleaned.chars().any(|c| matches!(c, '+' | '-' | '*' | '/' | '^' | '%'))
                || cleaned.contains("sqrt")
                || cleaned.contains("avg")
                || cleaned.contains("sum")
            {
                return Some(cleaned.to_string());
            }
        }
    }

    // Direct expression check (e.g. "((145.2 - 120.0) / 120.0) * 100")
    let has_operators = text.chars().any(|c| matches!(c, '+' | '*' | '/' | '^'));
    let has_digits = text.chars().any(|c| c.is_ascii_digit());
    if has_operators && has_digits && text.len() < 100 {
        let stripped = text.trim_end_matches('?').trim_end_matches('.').trim();
        if eval_math(stripped).is_ok() {
            return Some(stripped.to_string());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eval_basic_arithmetic() {
        assert_eq!(eval_math("2 + 2").unwrap(), 4.0);
        assert_eq!(eval_math("10 - 3 * 2").unwrap(), 4.0);
        assert_eq!(eval_math("(10 - 3) * 2").unwrap(), 14.0);
        assert_eq!(eval_math("10 / 2 + 5").unwrap(), 10.0);
        assert_eq!(eval_math("2 ^ 3").unwrap(), 8.0);
        assert_eq!(eval_math("10 % 3").unwrap(), 1.0);
    }

    #[test]
    fn test_eval_percentages() {
        // 20% of 150 = 30
        assert_eq!(eval_math("20% of 150").unwrap(), 30.0);
        assert_eq!(eval_math("15% * 2400").unwrap(), 360.0);
        // Margin / delta calculation: ((120 - 95) / 95) * 100
        let delta = eval_math("((120 - 95) / 95) * 100").unwrap();
        assert!((delta - 26.31578).abs() < 1e-4);
    }

    #[test]
    fn test_eval_functions() {
        assert_eq!(eval_math("sqrt(16)").unwrap(), 4.0);
        assert_eq!(eval_math("abs(-42.5)").unwrap(), 42.5);
        assert_eq!(eval_math("min(5, 2, 8)").unwrap(), 2.0);
        assert_eq!(eval_math("max(5, 2, 8)").unwrap(), 8.0);
        assert_eq!(eval_math("sum(10, 20, 30)").unwrap(), 60.0);
        assert_eq!(eval_math("avg(10, 20, 30)").unwrap(), 20.0);
    }

    #[test]
    fn test_division_by_zero() {
        assert!(eval_math("10 / 0").is_err());
        assert!(eval_math("sqrt(-1)").is_err());
    }

    #[test]
    fn test_find_math_intent() {
        assert_eq!(
            find_math_intent("Calculate 25% of 800?"),
            Some("25% of 800".to_string())
        );
        assert_eq!(
            find_math_intent("What is ((145.2 - 120.0) / 120.0) * 100?"),
            Some("((145.2 - 120.0) / 120.0) * 100".to_string())
        );
        assert_eq!(
            find_math_intent("What is the refund policy?"),
            None
        );
    }
}

//! A small, dependency-free arithmetic and comparison evaluator.
//!
//! It is used by `core.Calculate` and by edge guards. Writing it here rather than
//! pulling in a general expression crate keeps the dependency graph free of
//! unmaintained transitive packages and makes the accepted grammar an explicit,
//! documented part of the product.
//!
//! ## Grammar
//!
//! ```text
//! expression := or
//! or         := and ( '||' and )*
//! and        := equality ( '&&' equality )*
//! equality   := comparison ( ('==' | '!=') comparison )*
//! comparison := term ( ('<' | '<=' | '>' | '>=') term )*
//! term       := factor ( ('+' | '-') factor )*
//! factor     := unary ( ('*' | '/' | '%') unary )*
//! unary      := ('-' | '+' | '!') unary | power
//! power      := primary ( '^' unary )?
//! primary    := number | identifier | 'true' | 'false' | '(' expression ')'
//! ```
//!
//! Booleans evaluate to `1.0`/`0.0`, which lets a guard such as
//! `retries > 3 || failed` be used directly as an edge condition.

use std::collections::BTreeMap;
use std::fmt;

/// Failure raised while lexing, parsing or evaluating an expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExprError {
    /// A character that is not part of the grammar.
    UnexpectedChar(char),
    /// A token appeared where it is not valid.
    UnexpectedToken(String),
    /// The expression ended before it was complete.
    UnexpectedEnd,
    /// A variable was referenced but not bound.
    UnknownVariable(String),
    /// Division or remainder by zero.
    DivisionByZero,
}

impl fmt::Display for ExprError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedChar(c) => write!(f, "unexpected character `{c}`"),
            Self::UnexpectedToken(t) => write!(f, "unexpected token `{t}`"),
            Self::UnexpectedEnd => write!(f, "unexpected end of expression"),
            Self::UnknownVariable(name) => write!(f, "unknown variable `{name}`"),
            Self::DivisionByZero => write!(f, "division by zero"),
        }
    }
}

impl std::error::Error for ExprError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Pow,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
    And,
    Or,
    Not,
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f64),
    Ident(String),
    LParen,
    RParen,
    Op(Op),
}

fn lex(input: &str) -> Result<Vec<Token>, ExprError> {
    let chars: Vec<char> = input.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c.is_ascii_digit()
            || (c == '.' && matches!(chars.get(i + 1), Some(d) if d.is_ascii_digit()))
        {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            let number = text
                .parse::<f64>()
                .map_err(|_| ExprError::UnexpectedToken(text.clone()))?;
            tokens.push(Token::Number(number));
            continue;
        }
        if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len()
                && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '.')
            {
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            tokens.push(Token::Ident(text));
            continue;
        }
        if c == '(' {
            tokens.push(Token::LParen);
            i += 1;
            continue;
        }
        if c == ')' {
            tokens.push(Token::RParen);
            i += 1;
            continue;
        }
        let next = chars.get(i + 1).copied();
        let (op, width) = match (c, next) {
            ('|', Some('|')) => (Op::Or, 2),
            ('&', Some('&')) => (Op::And, 2),
            ('=', Some('=')) => (Op::Eq, 2),
            ('!', Some('=')) => (Op::Ne, 2),
            ('<', Some('=')) => (Op::Le, 2),
            ('>', Some('=')) => (Op::Ge, 2),
            ('+', _) => (Op::Add, 1),
            ('-', _) => (Op::Sub, 1),
            ('*', _) => (Op::Mul, 1),
            ('/', _) => (Op::Div, 1),
            ('%', _) => (Op::Rem, 1),
            ('^', _) => (Op::Pow, 1),
            ('<', _) => (Op::Lt, 1),
            ('>', _) => (Op::Gt, 1),
            ('!', _) => (Op::Not, 1),
            _ => return Err(ExprError::UnexpectedChar(c)),
        };
        tokens.push(Token::Op(op));
        i += width;
    }
    Ok(tokens)
}

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
    variables: &'a BTreeMap<String, serde_json::Value>,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn next(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.pos).cloned();
        if token.is_some() {
            self.pos += 1;
        }
        token
    }

    fn eat_op(&mut self, op: Op) -> bool {
        if matches!(self.peek(), Some(Token::Op(found)) if *found == op) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn parse_expression(&mut self) -> Result<f64, ExprError> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<f64, ExprError> {
        let mut left = self.parse_and()?;
        while self.eat_op(Op::Or) {
            let right = self.parse_and()?;
            left = bool_to_number(truthy(left) || truthy(right));
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<f64, ExprError> {
        let mut left = self.parse_equality()?;
        while self.eat_op(Op::And) {
            let right = self.parse_equality()?;
            left = bool_to_number(truthy(left) && truthy(right));
        }
        Ok(left)
    }

    fn parse_equality(&mut self) -> Result<f64, ExprError> {
        let mut left = self.parse_comparison()?;
        loop {
            if self.eat_op(Op::Eq) {
                let right = self.parse_comparison()?;
                left = bool_to_number((left - right).abs() < f64::EPSILON);
            } else if self.eat_op(Op::Ne) {
                let right = self.parse_comparison()?;
                left = bool_to_number((left - right).abs() >= f64::EPSILON);
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_comparison(&mut self) -> Result<f64, ExprError> {
        let mut left = self.parse_term()?;
        while let Some(Token::Op(op @ (Op::Lt | Op::Le | Op::Gt | Op::Ge))) = self.peek() {
            let op = *op;
            self.pos += 1;
            let right = self.parse_term()?;
            left = bool_to_number(match op {
                Op::Lt => left < right,
                Op::Le => left <= right,
                Op::Gt => left > right,
                Op::Ge => left >= right,
                _ => unreachable!("comparison operator filtered above"),
            });
        }
        Ok(left)
    }

    fn parse_term(&mut self) -> Result<f64, ExprError> {
        let mut left = self.parse_factor()?;
        loop {
            if self.eat_op(Op::Add) {
                left += self.parse_factor()?;
            } else if self.eat_op(Op::Sub) {
                left -= self.parse_factor()?;
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_factor(&mut self) -> Result<f64, ExprError> {
        let mut left = self.parse_unary()?;
        loop {
            if self.eat_op(Op::Mul) {
                left *= self.parse_unary()?;
            } else if self.eat_op(Op::Div) {
                let right = self.parse_unary()?;
                if right == 0.0 {
                    return Err(ExprError::DivisionByZero);
                }
                left /= right;
            } else if self.eat_op(Op::Rem) {
                let right = self.parse_unary()?;
                if right == 0.0 {
                    return Err(ExprError::DivisionByZero);
                }
                left %= right;
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<f64, ExprError> {
        if self.eat_op(Op::Sub) {
            return Ok(-self.parse_unary()?);
        }
        if self.eat_op(Op::Add) {
            return self.parse_unary();
        }
        if self.eat_op(Op::Not) {
            let value = self.parse_unary()?;
            return Ok(bool_to_number(!truthy(value)));
        }
        self.parse_power()
    }

    fn parse_power(&mut self) -> Result<f64, ExprError> {
        let base = self.parse_primary()?;
        if self.eat_op(Op::Pow) {
            let exponent = self.parse_unary()?;
            return Ok(base.powf(exponent));
        }
        Ok(base)
    }

    fn parse_primary(&mut self) -> Result<f64, ExprError> {
        match self.next() {
            Some(Token::Number(number)) => Ok(number),
            Some(Token::Ident(name)) => match name.as_str() {
                "true" => Ok(1.0),
                "false" => Ok(0.0),
                "pi" => Ok(std::f64::consts::PI),
                _ => self.resolve(&name),
            },
            Some(Token::LParen) => {
                let value = self.parse_expression()?;
                match self.next() {
                    Some(Token::RParen) => Ok(value),
                    Some(other) => Err(ExprError::UnexpectedToken(format!("{other:?}"))),
                    None => Err(ExprError::UnexpectedEnd),
                }
            }
            Some(other) => Err(ExprError::UnexpectedToken(format!("{other:?}"))),
            None => Err(ExprError::UnexpectedEnd),
        }
    }

    fn resolve(&self, name: &str) -> Result<f64, ExprError> {
        let mut segments = name.split('.');
        let head = segments.next().unwrap_or(name);
        let mut current = self
            .variables
            .get(head)
            .ok_or_else(|| ExprError::UnknownVariable(name.to_string()))?;
        for segment in segments {
            current = current
                .get(segment)
                .ok_or_else(|| ExprError::UnknownVariable(name.to_string()))?;
        }
        if let Some(number) = current.as_f64() {
            return Ok(number);
        }
        if let Some(boolean) = current.as_bool() {
            return Ok(bool_to_number(boolean));
        }
        Err(ExprError::UnknownVariable(name.to_string()))
    }
}

fn truthy(value: f64) -> bool {
    value != 0.0 && !value.is_nan()
}

fn bool_to_number(value: bool) -> f64 {
    if value {
        1.0
    } else {
        0.0
    }
}

/// Evaluate `expr` with the numeric subset of `variables` bound.
///
/// Variables holding numbers, booleans or nested objects of the same are
/// visible; anything else is treated as unbound.
pub fn evaluate_expression(
    expr: &str,
    variables: &BTreeMap<String, serde_json::Value>,
) -> Result<f64, ExprError> {
    let trimmed = expr.trim();
    if trimmed.is_empty() {
        return Err(ExprError::UnexpectedEnd);
    }
    let tokens = lex(trimmed)?;
    let mut parser = Parser {
        tokens: &tokens,
        pos: 0,
        variables,
    };
    let value = parser.parse_expression()?;
    if parser.pos != tokens.len() {
        return Err(ExprError::UnexpectedToken(format!(
            "{:?}",
            tokens[parser.pos]
        )));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn vars() -> BTreeMap<String, serde_json::Value> {
        let mut variables = BTreeMap::new();
        variables.insert("count".to_string(), json!(5));
        variables.insert("flag".to_string(), json!(true));
        variables.insert("nested".to_string(), json!({ "inner": 3 }));
        variables.insert("name".to_string(), json!("not a number"));
        variables
    }

    #[test]
    fn evaluates_arithmetic_with_precedence() {
        assert_eq!(evaluate_expression("2 + 2 * 3", &vars()).unwrap(), 8.0);
        assert_eq!(evaluate_expression("(2 + 2) * 3", &vars()).unwrap(), 12.0);
        assert_eq!(evaluate_expression("2 ^ 3 ^ 2", &vars()).unwrap(), 512.0);
        assert_eq!(evaluate_expression("7 % 4", &vars()).unwrap(), 3.0);
    }

    #[test]
    fn binds_variables_and_booleans() {
        assert_eq!(evaluate_expression("count * 2", &vars()).unwrap(), 10.0);
        assert_eq!(evaluate_expression("flag", &vars()).unwrap(), 1.0);
        assert_eq!(evaluate_expression("!flag", &vars()).unwrap(), 0.0);
    }

    #[test]
    fn resolves_dotted_paths() {
        assert_eq!(
            evaluate_expression("nested.inner + 1", &vars()).unwrap(),
            4.0
        );
    }

    #[test]
    fn evaluates_comparisons_and_logic() {
        assert_eq!(
            evaluate_expression("count > 3 && count < 10", &vars()).unwrap(),
            1.0
        );
        assert_eq!(
            evaluate_expression("count > 100 || flag", &vars()).unwrap(),
            1.0
        );
        assert_eq!(evaluate_expression("count == 5", &vars()).unwrap(), 1.0);
        assert_eq!(evaluate_expression("count != 5", &vars()).unwrap(), 0.0);
    }

    #[test]
    fn unary_minus_and_float_literals() {
        assert_eq!(evaluate_expression("-3.5 + 0.5", &vars()).unwrap(), -3.0);
    }

    #[test]
    fn rejects_unknown_variables() {
        assert_eq!(
            evaluate_expression("missing + 1", &vars()),
            Err(ExprError::UnknownVariable("missing".to_string()))
        );
        assert_eq!(
            evaluate_expression("name + 1", &vars()),
            Err(ExprError::UnknownVariable("name".to_string()))
        );
    }

    #[test]
    fn rejects_malformed_input() {
        assert_eq!(
            evaluate_expression("", &vars()),
            Err(ExprError::UnexpectedEnd)
        );
        assert_eq!(
            evaluate_expression("1 +", &vars()),
            Err(ExprError::UnexpectedEnd)
        );
        assert_eq!(
            evaluate_expression("1 @ 2", &vars()),
            Err(ExprError::UnexpectedChar('@'))
        );
        assert_eq!(
            evaluate_expression("(1 + 2", &vars()),
            Err(ExprError::UnexpectedEnd)
        );
        assert_eq!(
            evaluate_expression("1 / 0", &vars()),
            Err(ExprError::DivisionByZero)
        );
        assert_eq!(
            evaluate_expression("1 2", &vars()),
            Err(ExprError::UnexpectedToken("Number(2.0)".to_string()))
        );
    }
}

use std::ops::Range;

use anyhow::anyhow;
use logos::Logos;

pub trait HumanReadableToken {
    fn human_readable(&self) -> String;
}

#[derive(Logos, Debug, Copy, Clone, PartialEq)]
#[logos(skip r"[ \t\n\f]+")] // Ignore this regex pattern between tokens
pub enum Token {
    #[token(";")]
    Semicolon,
    #[regex("\"([^\"\\\\]+|\\\\\\\"|\\\\\\\\)*\"", priority = 100)]
    Quoted,
    // great regex support, 10/10 would recommend, totally not a workaround >.<
    #[regex("[^ \t\n\r;]+", |lex| {
        if lex.slice().starts_with('\"') || lex.slice().ends_with('\"') {
            return Err(());
        }
        Ok(())
    }, priority = 0)]
    Text,
}

impl HumanReadableToken for Token {
    fn human_readable(&self) -> String {
        format!(
            "\"{}\"",
            match self {
                Token::Semicolon => ";",
                Token::Text => "text",
                Token::Quoted => "quoted expression",
            }
        )
    }
}

pub fn token_err(s: &str) -> Option<anyhow::Error> {
    if s.trim().starts_with('"') {
        Some(anyhow!("Expected closing \""))
    } else {
        None
    }
}

pub type Tokens = Vec<(Token, String, Range<usize>)>;
pub fn tokenize(s: &str) -> anyhow::Result<Tokens, (Tokens, (String, Range<usize>))> {
    let mut lexer = Token::lexer(s);
    let mut res: Tokens = Default::default();

    while let Some(token) = lexer.next() {
        match token {
            Ok(token) => res.push((token, lexer.slice().to_string(), lexer.span())),
            Err(_) => {
                return Err((res, (lexer.slice().to_string(), lexer.span())));
            }
        }
    }
    Ok(res)
}

#[cfg(test)]
mod test {
    use logos::Logos;

    use crate::tokenizer::{Token, tokenize};

    #[test]
    fn console_tests() {
        let mut lex = Token::lexer("5");

        assert_eq!(lex.next(), Some(Ok(Token::Text)));
        assert_eq!(lex.slice(), "5");
        assert_eq!(lex.next(), None);

        let mut lex = Token::lexer("cl.map \"name with spaces\"");

        assert_eq!(lex.next(), Some(Ok(Token::Text)));
        assert_eq!(lex.slice(), "cl.map");

        assert_eq!(lex.next(), Some(Ok(Token::Quoted)));
        assert_eq!(lex.slice(), "\"name with spaces\"");

        assert_eq!(lex.next(), None);

        let mut lex = Token::lexer("+toggle cl.refresh_rate -0 +1000");

        assert_eq!(lex.next(), Some(Ok(Token::Text)));
        assert_eq!(lex.slice(), "+toggle");

        assert_eq!(lex.next(), Some(Ok(Token::Text)));
        assert_eq!(lex.slice(), "cl.refresh_rate");

        assert_eq!(lex.next(), Some(Ok(Token::Text)));
        assert_eq!(lex.slice(), "-0");

        assert_eq!(lex.next(), Some(Ok(Token::Text)));
        assert_eq!(lex.slice(), "+1000");

        assert_eq!(lex.next(), None);

        let mut lex = Token::lexer("\"test\\\"test\"");

        assert_eq!(lex.next(), Some(Ok(Token::Quoted)));
        assert_eq!(lex.slice(), "\"test\\\"test\"");

        assert_eq!(lex.next(), None);

        let mut lex = Token::lexer("{\"test\":5,\"test2\":false,\"test3\":\"str\"}");
        assert_eq!(lex.next(), Some(Ok(Token::Text)));
        assert_eq!(lex.next(), None);
        dbg!(&lex.collect::<Vec<_>>());

        // same but with spaces
        let mut lex = Token::lexer("{ \"test\" :5,\"test2\":false,\"test3\":\"str\"}");

        assert_eq!(lex.next(), Some(Ok(Token::Text)));
        assert_eq!(lex.next(), Some(Ok(Token::Quoted)));
        assert_eq!(lex.next(), Some(Ok(Token::Text)));
        assert_eq!(lex.next(), None);
        dbg!(&lex.collect::<Vec<_>>());

        let mut lex = Token::lexer("\"name with spaces\"d");

        assert_eq!(lex.next(), Some(Ok(Token::Quoted)));
        assert_eq!(lex.slice(), "\"name with spaces\"");

        assert_eq!(lex.next(), Some(Ok(Token::Text)));
        assert_eq!(lex.slice(), "d");

        assert_eq!(lex.next(), None);

        let mut lex = Token::lexer("player[0].name");

        assert_eq!(lex.next(), Some(Ok(Token::Text)));
        assert_eq!(lex.slice(), "player[0].name");

        assert_eq!(lex.next(), None);

        let tokens = tokenize("cl.refresh_rate;player").unwrap();
        let mut tokens = tokens.into_iter();
        assert_eq!(
            tokens.next(),
            Some((Token::Text, "cl.refresh_rate".to_string(), 0..15))
        );
        assert_eq!(
            tokens.next(),
            Some((Token::Semicolon, ";".to_string(), 15..16))
        );
        assert_eq!(
            tokens.next(),
            Some((Token::Text, "player".to_string(), 16..22))
        );

        // some unicode
        let mut lex = Token::lexer("™");

        assert_eq!(lex.next(), Some(Ok(Token::Text)));
        assert_eq!(lex.slice(), "™");
        assert_eq!(lex.span(), 0..3);
        assert_eq!(lex.next(), None);
    }

    #[test]
    fn err_console_tests() {
        let (tokens, (broken_token, _)) = tokenize("cl.map \"name with spaces").unwrap_err();
        let mut tokens = tokens.into_iter();

        assert_eq!(
            tokens.next(),
            Some((Token::Text, "cl.map".to_string(), 0..6))
        );

        assert_eq!(tokens.next(), None);

        assert_eq!(broken_token, "\"name with spaces");

        // \" only at end
        let (tokens, (broken_token, _)) = tokenize("cl.map name with spaces\"").unwrap_err();
        let mut tokens = tokens.into_iter();

        assert_eq!(
            tokens.next(),
            Some((Token::Text, "cl.map".to_string(), 0..6))
        );

        assert_eq!(
            tokens.next(),
            Some((Token::Text, "name".to_string(), 7..11))
        );
        assert_eq!(
            tokens.next(),
            Some((Token::Text, "with".to_string(), 12..16))
        );

        assert_eq!(broken_token, "spaces\"");
    }
}

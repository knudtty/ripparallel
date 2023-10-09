#[derive(Debug, PartialEq, Eq)]
pub enum Token {
    Literal(String),
    Substitute,
}

impl Token {
    pub fn get_tokens(input: String, quotes: bool) -> Vec<Token> {
        let mut lexer = Lexer::new(input);
        if quotes {
            let mut out = Vec::new();
            let mut open_quote = false;
            let mut token_idx = 0;
            while let Some(token) = lexer.next_token() {
                match token {
                    Token::Literal(l) => {
                        if open_quote {
                            let mut s = String::with_capacity(l.len());
                            s.push('\'');
                            s.push_str(l.as_str());
                            out.push(Token::Literal(s));
                        } else {
                            out.push(Token::Literal(l));
                        }
                    }
                    Token::Substitute => {
                        if token_idx > 1 {
                            match out.get_mut(token_idx - 1) {
                                Some(Token::Literal(l)) => {
                                    if l.get(l.len() - 1..) != Some("\'") {
                                        l.push('\'');
                                        open_quote = true;
                                    }
                                }
                                _ => {}
                            }
                        } else {
                            out.push(Token::Substitute)
                        }
                    }
                }
                token_idx += 1;
            }
            out
        } else {
            let mut out = Vec::new();
            while let Some(token) = lexer.next_token() {
                out.push(token);
            }
            out
        }
    }
}

pub struct Lexer {
    input: String,
    position: usize,
    read_position: usize,
    ch: u8,
}

impl Lexer {
    fn new(input: String) -> Self {
        let mut l = Lexer {
            input,
            position: 0,
            read_position: 0,
            ch: 0,
        };
        l.read_char();
        l
    }

    fn read_char(&mut self) {
        if self.read_position >= self.input.len() {
            self.ch = 0;
        } else {
            self.ch = self.input.as_bytes()[self.read_position];
        }
        self.position = self.read_position;
        self.read_position += 1;
    }

    fn next_token(&mut self) -> Option<Token> {
        return match self.ch {
            b'{' => Some(self.read_brackets()),
            0 => None,
            _ => Some(Token::Literal(self.read_literal())),
        };
    }

    fn read_literal(&mut self) -> String {
        let position = self.position;
        loop {
            match self.ch {
                b'{' => {
                    if Some(b'\\') == self.input.bytes().nth(self.read_position - 1) {
                        self.read_char();
                    } else {
                        break;
                    }
                }
                0 => break,
                _ => self.read_char(),
            }
        }
        let out = self.input[position..self.position].to_owned();
        return out;
    }

    fn read_brackets(&mut self) -> Token {
        let position = self.position;
        loop {
            match self.ch {
                b'}' => {
                    self.read_char();
                    return Token::Substitute
                },
                0 => break,
                // TODO: expand to arguments inside {}
                _ => self.read_char()
            }
        }
        // if closing } is never encountered
        return Token::Literal(self.input[position..self.position].to_owned());
    }
}

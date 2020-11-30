use regex::Regex;
use ron::to_string;
use std::ops::Range;
use std::rc::Rc;

pub enum ParserKind {
    Literal(String),
    Regex(Regex),
    Constant(String),
    And,
    Ignore(bool),
    Or,
    Repeat(usize),
    RepeatRange(Range<usize>),
    Error(String),
    Map(Rc<Box<dyn Fn(String) -> Result<String, ron::Error>>>),
}
impl std::fmt::Debug for ParserKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}
impl std::fmt::Display for ParserKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use ParserKind::*;
        match self {
            Literal(s) => write!(f, "Literal \"{}\"", s),
            Regex(r) => write!(f, "Regex /{}/", r.as_str()),
            Constant(c) => write!(f, "Constant \"{}\"", c),
            And => write!(f, "And"),
            Ignore(b) => write!(f, "Ignore{}", if *b { "Before" } else { "After" }),
            Or => write!(f, "Or"),
            Repeat(num) => write!(f, "Repeat {}", num),
            RepeatRange(range) => write!(f, "RepeatRange {:?}", range),
            Error(msg) => write!(f, "Error \"{}\"", msg),
            Map(_) => write!(f, "Map"),
        }
    }
}
impl Clone for ParserKind {
    fn clone(&self) -> Self {
        use ParserKind::*;
        match self {
            Literal(s) => Literal(s.clone()),
            Regex(r) => Regex(r.clone()),
            Constant(c) => Constant(c.clone()),
            And => And,
            Ignore(b) => Ignore(*b),
            Or => Or,
            Repeat(num) => Repeat(num.clone()),
            RepeatRange(range) => RepeatRange(range.clone()),
            Error(msg) => Error(msg.clone()),
            Map(cfn) => Map(Rc::clone(cfn)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Parser {
    kind: ParserKind,
    subparsers: Vec<Parser>,
}
impl std::fmt::Display for Parser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.pretty_print(f, 0)
    }
}
impl Parser {
    pub fn parse<T: Into<String>>(&self, src: T) -> Result<(String, String), String> {
        use ParserKind::*;
        let s: String = src.into();
        match &self.kind {
            Literal(literal) => {
                if s.len() >= literal.len() && s[..literal.len()] == literal[..] {
                    Ok((s[..literal.len()].to_owned(), s[literal.len()..].to_owned()))
                } else {
                    Err(s)
                }
            }
            Regex(re) => {
                if let Some(mat) = re.find(&s) {
                    if mat.start() == 0 {
                        Ok((
                            s[mat.start()..mat.end()].to_owned(),
                            s[mat.end()..].to_owned(),
                        ))
                    } else {
                        Err(s)
                    }
                } else {
                    Err(s)
                }
            }
            Constant(constant) => Ok((constant.clone(), s)),
            And => {
                let (lmatched, lrest) = self.subparsers[0].parse(s)?;
                let (rmatched, rrest) = self.subparsers[1].parse(lrest)?;
                Ok((
                    to_string(&vec![lmatched.clone(), rmatched.clone()]).unwrap(),
                    rrest,
                ))
            }
            Ignore(before) => {
                if *before {
                    let (_, rest) = self.subparsers[0].parse(s)?;
                    self.subparsers[1].parse(rest)
                } else {
                    let (matched, rest) = self.subparsers[0].parse(s)?;
                    let (_, rest) = self.subparsers[1].parse(rest)?;
                    Ok((matched, rest))
                }
            }
            Or => {
                if let Ok(lresult) = self.subparsers[0].parse(s.clone()) {
                    Ok(lresult)
                } else {
                    self.subparsers[1].parse(s.clone())
                }
            }
            Repeat(num_repeats) => {
                let mut matched = vec![];
                let mut rest = s.clone();
                for _ in 0..*num_repeats {
                    let (m, r) = self.subparsers[0].parse(rest)?;
                    matched.push(m);
                    rest = r;
                }
                Ok((to_string(&matched).unwrap(), rest))
            }
            RepeatRange(range) => {
                let mut matched = vec![];
                let mut rest = s.clone();

                // Parse up to range.start
                for _ in 0..range.start {
                    let (m, r) = self.subparsers[0].parse(rest)?;
                    matched.push(m);
                    rest = r;
                }

                // Parse optionally up to range.end
                for _ in 0..(range.end - range.start) {
                    let parse_result = self.subparsers[0].parse(rest);
                    if let Err(r) = parse_result {
                        rest = r;
                        break;
                    } else {
                        let (m, r) = parse_result.unwrap();
                        matched.push(m);
                        rest = r;
                    }
                }

                Ok((to_string(&matched).unwrap(), rest))
            }
            Error(msg) => panic!(msg.clone()),
            Map(cfn) => {
                let (matched, rest) = self.subparsers[0].parse(s)?;
                if let Ok(m) = cfn(matched) {
                    Ok((m, rest))
                } else {
                    Err(rest)
                }
            }
        }
    }

    // Static
    pub fn literal<T: Into<String>>(s: T) -> Parser {
        Parser {
            kind: ParserKind::Literal(s.into()),
            subparsers: vec![],
        }
    }
    pub fn regex<T: Into<String>>(s: T) -> Parser {
        Parser {
            kind: ParserKind::Regex(Regex::new(&s.into()).expect("could not compile regex")),
            subparsers: vec![],
        }
    }
    pub fn constant<T: Into<String>>(s: T) -> Parser {
        Parser {
            kind: ParserKind::Constant(s.into()),
            subparsers: vec![],
        }
    }
    pub fn error<T: Into<String>>(s: T) -> Parser {
        Parser {
            kind: ParserKind::Error(s.into()),
            subparsers: vec![],
        }
    }

    // Instance
    pub fn and(self, r: Parser) -> Parser {
        Parser {
            kind: ParserKind::And,
            subparsers: vec![self, r],
        }
    }
    pub fn ignore_before(self, r: Parser) -> Parser {
        Parser {
            kind: ParserKind::Ignore(true),
            subparsers: vec![self, r],
        }
    }
    pub fn ignore_after(self, r: Parser) -> Parser {
        Parser {
            kind: ParserKind::Ignore(false),
            subparsers: vec![self, r],
        }
    }
    pub fn or(self, r: Parser) -> Parser {
        Parser {
            kind: ParserKind::Or,
            subparsers: vec![self, r],
        }
    }
    pub fn repeat(self, num_repeats: usize) -> Parser {
        Parser {
            kind: ParserKind::Repeat(num_repeats),
            subparsers: vec![self],
        }
    }
    pub fn repeat_range(self, num_repeats: Range<usize>) -> Parser {
        Parser {
            kind: ParserKind::RepeatRange(num_repeats),
            subparsers: vec![self],
        }
    }
    pub fn optional(self) -> Parser {
        Parser {
            kind: ParserKind::RepeatRange(0..1),
            subparsers: vec![self],
        }
    }
    pub fn map<F: 'static>(self, cfn: F) -> Parser
    where
        F: Fn(String) -> Result<String, ron::Error>,
    {
        Parser {
            kind: ParserKind::Map(Rc::new(Box::new(cfn))),
            subparsers: vec![self],
        }
    }

    // Other
    pub fn pretty_print(&self, f: &mut std::fmt::Formatter<'_>, indent: usize) -> std::fmt::Result {
        for _ in 0..indent {
            write!(f, " ")?;
        }
        write!(f, "{}", self.kind)?;
        if self.subparsers.len() > 0 {
            write!(f, " [\n")?;
            for subparser in &self.subparsers {
                subparser.pretty_print(f, indent + 2)?;
                write!(f, ",\n")?;
            }
            for _ in 0..indent {
                write!(f, " ")?;
            }
            write!(f, "]")
        } else {
            write!(f, "")
        }
    }
}

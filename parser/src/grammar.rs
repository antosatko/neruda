use ruparse::Parser;
use ruparse::api::ext::*;
use ruparse::grammar::validator::Validator;
use ruparse::grammar::{ErrorDefinition, MatchToken, VarKind, VariableKind};
pub use ruparse::lexer::Token;
use ruparse::lexer::{ControlTokenKind, PreprocessorError, TokenKinds};

const IDENTIFIER: VarKind<'static> = local("identifier");
const IDENTIFIER_VAR: (&'static str, VariableKind) = ("identifier", VariableKind::Node);

const KEYWORDS: &[&'static str] = &[
    "systems",
    "resources",
    "scheduler",
    "system",
    "struct",
    "component",
    "var",
    "function",
    "return",
    "if",
    "else",
    "loop",
    "while",
    "break",
    "continue",
];

static _ERR: ErrorDefinition = ErrorDefinition {
    header: "------",
    code: "200",
    msg: "-------------",
};

static UNCLOSED_STRING_LIT: ErrorDefinition = ErrorDefinition {
    header: "Unclosed string",
    code: "200",
    msg: "Expected string literal to end before the end of file",
};

static EMPTY_CHAR_LIT: ErrorDefinition = ErrorDefinition {
    header: "Empty character",
    code: "201",
    msg: "Expected character literal to not be empty",
};

static MULTIPLE_TRAILING_COMMAS: ErrorDefinition = ErrorDefinition {
    header: "Multiple trailing commas",
    code: "202",
    msg: "Only one trailing comma allowed",
};

static EMPTY_INDEXING: ErrorDefinition = ErrorDefinition {
    header: "Empty indexing",
    code: "203",
    msg: "Expected an expression to index with",
};

static CHARACTER_OVERFLOW: ErrorDefinition = ErrorDefinition {
    header: "Character overflow",
    code: "204",
    msg: "Expected character literal to contain a single character",
};

static UNCLOSED_CHAR_LIT: ErrorDefinition = ErrorDefinition {
    header: "Unclosed character",
    code: "205",
    msg: "Unclosed character literal",
};

static UNKNOWN_CHARACTER: ErrorDefinition = ErrorDefinition {
    header: "Unknown character",
    code: "206",
    msg: "Unknown character literal",
};

static EXPECTED_UNICODE: ErrorDefinition = ErrorDefinition {
    header: "Expected Unicode",
    code: "207",
    msg: "Expected a unicode number in unicode escape sequence. Example: \u{0b0101}",
};

fn keyword(kw: &'static str) -> MatchToken<'static> {
    assert!(KEYWORDS.contains(&kw), "{kw} must be a keyword");
    word(kw)
}

fn is_numeric(str: &str) -> bool {
    str.chars()
        .nth(0)
        .expect("Expected a non empty text")
        .is_numeric()
}

fn count_hashes(tokens: &[Token]) -> usize {
    tokens
        .iter()
        .take_while(|t| t.kind == TokenKinds::Token("#"))
        .count()
}

pub fn gen_parser<'src>() -> Parser<'static> {
    let mut parser = Parser::new();

    parser.lexer.add_tokens(
        "+ - * / \\ ; \" ' : :: ( { [ < > ] } ) | & ! ? = . , # == != += -= *= /= %= && || >= <= =>"
            .split_whitespace(),
    );
    parser.grammar.ignored.push(TokenKinds::Complex("comment"));
    parser.lexer.preprocessors.push(|src, tokens| {
        use ruparse::lexer::TokenKinds;
        let mut i = 0;
        let mut result = Vec::new();

        while i < tokens.len() {
            let tok = &tokens[i];
            match &tok.kind {
                TokenKinds::Text => {
                    let numeric = is_numeric(tok.stringify(src));
                    if !numeric {
                        result.push(*tok);
                        i += 1;
                        continue;
                    }
                    if let Some(t) = tokens.get(i + 1)
                        && t.kind == TokenKinds::Token(".")
                    {
                        if let Some(t) = tokens.get(i + 2)
                            && t.kind == TokenKinds::Text
                            && is_numeric(t.stringify(src))
                        {
                            let num = Token {
                                index: tok.index,
                                len: tok.len + 1 + t.len,
                                location: tok.location,
                                kind: TokenKinds::Complex("float"),
                            };
                            result.push(num);
                            i += 3;
                            continue;
                        }
                    }
                    let num = Token {
                        index: tok.index,
                        len: tok.len,
                        location: tok.location,
                        kind: TokenKinds::Complex("numeric"),
                    };
                    result.push(num);
                }
                _ => result.push(*tok),
            }
            i += 1;
        }

        Ok(result)
    });

    parser.lexer.preprocessors.push(|src, tokens| {
        use ruparse::lexer::TokenKinds;
        let mut i = 0;
        let mut result = Vec::new();

        'main: while i < tokens.len() {
            let tok = &tokens[i];
            match &tok.kind {
                // might be a comment
                TokenKinds::Token(t) if *t == "/" => {
                    let start = i;
                    if let Some(t) = tokens.get(i + 1)
                        && t.kind == TokenKinds::Token("/")
                    {
                        if let Some(t) = tokens.get(i + 2)
                            && t.kind == TokenKinds::Token("/")
                        {
                            // is a documentation comment
                            while let Some(t) = tokens.get(i)
                                && t.kind != TokenKinds::Control(ControlTokenKind::Eol)
                            {
                                i += 1;
                            }
                            let tok = Token {
                                index: tokens[start].index,
                                len: tokens[i].index - tokens[start].index,
                                location: tokens[start].location,
                                kind: TokenKinds::Complex("docstr"),
                            };
                            result.push(tok);
                            continue;
                        }
                        if let Some(t) = tokens.get(i + 2)
                            && t.kind == TokenKinds::Token("!")
                        {
                            // is a top level documentation comment
                            while let Some(t) = tokens.get(i)
                                && t.kind != TokenKinds::Control(ControlTokenKind::Eol)
                            {
                                i += 1;
                            }
                            let tok = Token {
                                index: tokens[start].index,
                                len: tokens[i].index - tokens[start].index,
                                location: tokens[start].location,
                                kind: TokenKinds::Complex("tl docstr"),
                            };
                            result.push(tok);
                            continue;
                        } else {
                            // is a comment
                            while let Some(t) = tokens.get(i)
                                && t.kind != TokenKinds::Control(ControlTokenKind::Eol)
                            {
                                i += 1;
                            }
                            let tok = Token {
                                index: tokens[start].index,
                                len: tokens[i].index - tokens[start].index,
                                location: tokens[start].location,
                                kind: TokenKinds::Complex("comment"),
                            };
                            result.push(tok);
                            continue;
                        }
                    }
                    result.push(tokens[i]);
                }
                TokenKinds::Token(t) if *t == "'" => match tokens.get(i + 1).map(|t| &t.kind) {
                    Some(TokenKinds::Token(t)) if *t == "\\" => {
                        match tokens.get(i + 2).map(|t| t) {
                            Some(t) => {
                                let str = t.stringify(src);
                                let literal = if "\"\\\'0abfnrtv".contains(str) {
                                    // classic escapes
                                    let tok = Token {
                                        index: tok.index,
                                        len: tokens[i + 2].len + 3,
                                        location: tok.location,
                                        kind: TokenKinds::Complex("char"),
                                    };
                                    i += 3;
                                    tok
                                } else if str == "u" {
                                    // unicode
                                    match (
                                        tokens.get(i + 3).map(|t| &t.kind),
                                        tokens.get(i + 4).map(|t| &t.kind),
                                        tokens.get(i + 5).map(|t| &t.kind),
                                    ) {
                                        (
                                            Some(TokenKinds::Token(open)),
                                            Some(TokenKinds::Complex(code)),
                                            Some(TokenKinds::Token(close)),
                                        ) if *open == "{"
                                            && *code == "numeric"
                                            && *close == "}" =>
                                        {
                                            let tok = Token {
                                                index: tok.index,
                                                len: tokens[i + 4].len + 6,
                                                location: tok.location,
                                                kind: TokenKinds::Complex("char"),
                                            };
                                            i += 6;
                                            tok
                                        }
                                        (Some(TokenKinds::Token(op)), Some(_), _) if *op == "{" => {
                                            Err(PreprocessorError {
                                                err: EXPECTED_UNICODE,
                                                location: tokens[i + 4].location,
                                                len: tokens[i + 4].len,
                                            })?
                                        }
                                        _ => Err(PreprocessorError {
                                            err: EXPECTED_UNICODE,
                                            location: tokens[i + 2].location,
                                            len: tokens[i + 2].len,
                                        })?,
                                    }
                                } else {
                                    // unknown
                                    Err(PreprocessorError {
                                        err: UNKNOWN_CHARACTER,
                                        location: tokens[i + 1].location,
                                        len: t.len + 1,
                                    })?
                                };

                                match tokens.get(i).map(|t| &t.kind) {
                                    Some(TokenKinds::Token(t)) if *t == "'" => {
                                        result.push(literal);
                                        i += 1;
                                        continue;
                                    }
                                    _ => Err(PreprocessorError {
                                        err: UNCLOSED_CHAR_LIT,
                                        location: tok.location,
                                        len: t.len + 2,
                                    })?,
                                }
                            }
                            _ => Err(PreprocessorError {
                                err: UNCLOSED_CHAR_LIT,
                                location: tok.location,
                                len: tok.len + tokens[i + 1].len,
                            })?,
                        }
                    }
                    Some(TokenKinds::Text) => {
                        let chars = tokens[i + 1].stringify(src).chars();
                        if chars.count() > 1 {
                            Err(PreprocessorError {
                                err: CHARACTER_OVERFLOW,
                                location: tok.location,
                                len: tok.len + tokens[i + 1].len,
                            })?
                        }
                        match tokens.get(i + 2).map(|t| &t.kind) {
                            Some(TokenKinds::Token(t)) if *t == "'" => {
                                let tok = Token {
                                    index: tok.index,
                                    len: tokens[i + 1].len + 2,
                                    location: tok.location,
                                    kind: TokenKinds::Complex("char"),
                                };
                                result.push(tok);
                                i += 3;
                                continue;
                            }
                            _ => Err(PreprocessorError {
                                err: UNCLOSED_CHAR_LIT,
                                location: tok.location,
                                len: tok.len + tokens[i + 1].len,
                            })?,
                        }
                    }
                    Some(TokenKinds::Token(t)) if *t == "'" => Err(PreprocessorError {
                        err: EMPTY_CHAR_LIT,
                        location: tok.location,
                        len: 1,
                    })?,
                    Some(_) => Err(PreprocessorError {
                        err: UNKNOWN_CHARACTER,
                        location: tokens[i + 1].location,
                        len: tokens[i + 1].len,
                    })?,
                    None => Err(PreprocessorError {
                        err: UNCLOSED_CHAR_LIT,
                        location: tok.location,
                        len: 1,
                    })?,
                },
                TokenKinds::Token(t) if *t == "\"" => {
                    let mut offset = 1;
                    while let Some(token) = tokens.get(i + offset) {
                        match &token.kind {
                            TokenKinds::Token(t) if *t == "\"" => {
                                let token = Token {
                                    index: tok.index,
                                    len: token.index - tok.index + 1,
                                    location: tok.location,
                                    kind: TokenKinds::Complex("string"),
                                };
                                result.push(token);
                                i += offset + 1;
                                continue 'main;
                            }
                            TokenKinds::Token(t) if *t == "\\" => offset += 2,
                            _ => offset += 1,
                        }
                    }
                    Err(PreprocessorError {
                        err: UNCLOSED_STRING_LIT,
                        location: tok.location,
                        len: tok.len,
                    })?
                }
                TokenKinds::Token(t) if *t == "#" => {
                    let count = count_hashes(&tokens[i..]);
                    if let Some(t) = tokens.get(i + count)
                        && t.kind == TokenKinds::Token("\"")
                    {
                        let mut offset = count + 1;
                        while let Some(token) = tokens.get(i + offset) {
                            match &token.kind {
                                TokenKinds::Token(t) if *t == "\"" => {
                                    if count_hashes(&tokens[i + offset + 1..]) < count {
                                        i += 1;
                                        continue;
                                    }
                                    offset += count;
                                    let token = Token {
                                        index: tok.index,
                                        len: tokens[i + offset].index - tok.index + 1,
                                        location: tok.location,
                                        kind: TokenKinds::Complex("string"),
                                    };
                                    result.push(token);
                                    i += offset + 1;
                                    continue 'main;
                                }
                                TokenKinds::Token(t) if *t == "\\" => offset += 2,
                                _ => offset += 1,
                            }
                        }
                        Err(PreprocessorError {
                            err: UNCLOSED_STRING_LIT,
                            location: tok.location,
                            len: tok.len,
                        })?
                    }
                    result.push(*tok);
                }
                // TokenKinds::Whitespace => (),
                _ => result.push(*tok),
            }
            i += 1;
        }

        Ok(result)
    });

    let keywords = parser
        .grammar
        .new_enum("keyword")
        .options(KEYWORDS.iter().map(|kw| word(kw)))
        .build();

    let operators = parser
        .grammar
        .new_enum("operator")
        .options(
            "+ - * / == != < > <= >= || && += -= *= /= %= ="
                .split_whitespace()
                .map(|t| token(t)),
        )
        .build();

    let delimiters_peek_enum = parser
        .grammar
        .new_enum("delimiters")
        .options(") ] }".split_whitespace().map(|c| token(c)))
        .build();

    let delimiters_peek = parser
        .grammar
        .new_node("delimiter")
        .rules([peek(delimiters_peek_enum)])
        .build();

    let delimiters_consume = parser
        .grammar
        .new_enum("terminator")
        .options([token(";"), newline()])
        .build();

    let docstr = parser
        .grammar
        .new_node("doc string")
        .rules([while_(complex("docstr")).set("docstr")])
        .variables([list_var("docstr")])
        .build();

    let tl_docstr = parser
        .grammar
        .new_node("top level doc string")
        .rules([while_(complex("tl docstr")).set("docstr")])
        .variables([list_var("docstr")])
        .build();

    let ident = parser
        .grammar
        .new_node("identifier")
        .rules([
            isnt(keywords).hint("Keywords are reserved and can not be used for identifiers"),
            is(text()).set(IDENTIFIER),
        ])
        .variables([node_var("identifier")])
        .build();

    let ident_path = parser
        .grammar
        .new_node("identifier path")
        .rules([
            is(ident).set("path").commit(),
            while_(token("::")).then([is(ident)
                .set("path")
                .hint("Static path must end on an identifier")]),
        ])
        .variables([list_var("path")])
        .build();

    let array_literal = parser
        .grammar
        .new_node("array literal")
        .rules([
            is(token("[")).commit(),
            loop_().then([
                maybe(node("expression")).set("elements"),
                is_one_of([
                    option(token(",")).then([maybe(token(",")).fail(&MULTIPLE_TRAILING_COMMAS)]),
                    option(token("]")).return_node(),
                ]),
            ]),
        ])
        .variables([list_var("elements")])
        .build();

    let tuple_literal = parser
        .grammar
        .new_node("tuple literal")
        .rules([
            is(token("(")).commit(),
            loop_().then([
                maybe(node("expression")).set("expressions"),
                is_one_of([
                    option(token(",")).then([maybe(token(",")).fail(&MULTIPLE_TRAILING_COMMAS)]),
                    option(token(")")).return_node(),
                ]),
            ]),
        ])
        .variables([list_var("expressions")])
        .build();

    let literals = parser
        .grammar
        .new_enum("literal")
        .options([
            ident_path,
            complex("string"),
            complex("char"),
            array_literal,
            tuple_literal,
            complex("numeric"),
            complex("float"),
        ])
        .build();

    let field = parser
        .grammar
        .new_node("field access")
        .rules([is(token(".")), is(ident).commit().set("identifier")])
        .variables([node_var("identifier")])
        .build();

    let call = parser
        .grammar
        .new_node("function call")
        .rules([
            is(token("(")).commit(),
            loop_().then([
                maybe(node("expression")).set("expressions"),
                is_one_of([
                    option(token(",")).then([maybe(token(",")).fail(&MULTIPLE_TRAILING_COMMAS)]),
                    option(token(")")).return_node(),
                ]),
            ]),
        ])
        .variables([list_var("expressions")])
        .build();

    let indexing = parser
        .grammar
        .new_node("indexing")
        .rules([
            is(token("[")).commit(),
            is_one_of([
                option(token("]")).fail(&EMPTY_INDEXING),
                option(node("expression")).set("index"),
            ])
            .hint("Must index with a valid expression"),
            is(token("]")),
        ])
        .variables([node_var("index")])
        .build();

    let ref_tokens = parser
        .grammar
        .new_enum("ref token")
        .options([token("&"), token("&&"), token("*")])
        .build();

    let refs = parser
        .grammar
        .new_node("ref cast")
        .rules([
            is(token(".")),
            is(ref_tokens).set("refs").commit(),
            while_(ref_tokens).set("refs"),
        ])
        .variables([list_var("refs")])
        .build();

    let value_tails = parser
        .grammar
        .new_enum("value tail")
        .options([field, call, indexing, refs])
        .build();

    let unary_operators = parser
        .grammar
        .new_enum("unary operator")
        .options([token("-"), token("!")])
        .build();

    let value = parser
        .grammar
        .new_node("value")
        .rules([
            while_(unary_operators).set("unary operators"),
            is(literals).commit().set("literal"),
            while_(value_tails).set("tail"),
        ])
        .variables([
            node_var("literal"),
            list_var("tail"),
            list_var("unary operators"),
        ])
        .build();

    let expression = parser
        .grammar
        .new_node("expression")
        .rules([
            is(value).set("lvalue").commit(),
            while_(operators).set("rest").then([is(value)
                .set("rest")
                .hint("Binary operator must contain right value")]),
        ])
        .variables([node_var("lvalue"), list_var("rest")])
        .build();

    let end_stmt = parser
        .grammar
        .new_node("end statement")
        .rules([
            is_one_of([option(delimiters_consume), option(delimiters_peek)])
                .hint("Expected to end statement on a delimiter"),
        ])
        .build();

    let type_ = parser
        .grammar
        .new_node("type")
        .rules([is(ident_path).commit().set("path")])
        .variables([node_var("path")])
        .build();

    let label = parser
        .grammar
        .new_node("label")
        .rules([is(ident).set(IDENTIFIER).start(), is(token(":"))])
        .variables([IDENTIFIER_VAR])
        .build();

    let parameter = parser
        .grammar
        .new_node("parameter")
        .rules([
            is(ident).set("identifier").commit(),
            is(token(":")),
            is(type_).set("type"),
        ])
        .variables([node_var("identifier"), node_var("type")])
        .build();

    let parameter_list = parser
        .grammar
        .new_node("parameters")
        .rules([
            is(token("(")).commit(),
            loop_().then([
                maybe(parameter).set("parameters"),
                is_one_of([
                    option(token(",")).then([maybe(token(",")).fail(&MULTIPLE_TRAILING_COMMAS)]),
                    option(token(")")).return_node(),
                ]),
            ]),
        ])
        .variables([list_var("parameters")])
        .build();

    let expression_st = parser
        .grammar
        .new_node("expression statement")
        .rules([is(expression).set("expression").commit(), is(end_stmt)])
        .variables([node_var("expression")])
        .build();

    let variable_st = parser
        .grammar
        .new_node("variable")
        .rules([
            is(keyword("var")).commit(),
            is(ident).set(IDENTIFIER),
            maybe(token(":")).then([is(type_).set("type")]),
            maybe(token("=")).then([is(expression)
                .set("expression")
                .hint("Variable must be initialized to a valid expression")]),
            is(end_stmt),
        ])
        .variables([IDENTIFIER_VAR, node_var("type"), node_var("expression")])
        .build();

    let return_st = parser
        .grammar
        .new_node("return")
        .rules([
            is(keyword("return")).commit(),
            is(expression).set("expression"),
            is(end_stmt),
        ])
        .variables([node_var("expression")])
        .build();

    let continue_st = parser
        .grammar
        .new_node("continue")
        .maybe_has(label, "label")
        .rules([is(keyword("continue")).start()])
        .build();

    let break_st = parser
        .grammar
        .new_node("break")
        .maybe_has(label, "label")
        .rules([is(keyword("break")).start()])
        .build();

    let code_block = parser
        .grammar
        .new_node("code block")
        .rules([
            is(token("{")).commit(),
            loop_().then([is_one_of([
                option(enumerator("statement")).set("statements"),
                option(token("}")).return_node(),
            ])
            .hint("Potentially unclosed code body")]),
        ])
        .variables([list_var("statements")])
        .build();

    let code_expression = parser
        .grammar
        .new_node("code statement")
        .rules([
            is(token("=>")).commit(),
            is(expression_st).set("expression"),
        ])
        .variables([node_var("expression")])
        .build();

    let code_body = parser
        .grammar
        .new_enum("code body")
        .options([code_block, code_expression])
        .build();

    let loop_st = parser
        .grammar
        .new_node("loop")
        .maybe_has(label, "label")
        .rules([
            is(keyword("loop")).commit().start(),
            is(code_body).set("code body"),
        ])
        .variables([node_var("code body")])
        .build();

    let else_if_st = parser
        .grammar
        .new_node("else if")
        .rules([
            is(keyword("else")),
            is(keyword("if")).commit(),
            is(expression).set("expression"),
            is(code_body).set("code body"),
        ])
        .variables([node_var("expression"), node_var("code body")])
        .build();

    let else_st = parser
        .grammar
        .new_node("else")
        .rules([is(keyword("else")).commit(), is(code_body).set("code body")])
        .variables([node_var("code body")])
        .build();

    let if_st = parser
        .grammar
        .new_node("if")
        .rules([
            is(keyword("if")).commit(),
            is(expression).set("expression"),
            is(code_body).set("code body"),
            while_(else_if_st).set("else if"),
            maybe(else_st).set("else"),
        ])
        .variables([
            node_var("expression"),
            node_var("code body"),
            list_var("else if"),
            node_var("else"),
        ])
        .build();

    let while_st = parser
        .grammar
        .new_node("while")
        .maybe_has(label, "label")
        .rules([
            is(keyword("while")).commit().start(),
            is(expression).set("expression"),
            is(code_body).set("code body"),
        ])
        .variables([node_var("expression"), node_var("code body")])
        .build();

    // references
    let _ = code_body;
    let _statements = parser
        .grammar
        .new_enum("statement")
        .options([
            variable_st,
            return_st,
            continue_st,
            break_st,
            loop_st,
            if_st,
            while_st,
            expression_st,
        ])
        .build();

    let function = parser
        .grammar
        .new_node("function")
        .has(docstr, "docs")
        .rules([
            is(keyword("function")).commit().start(),
            is(ident).set(IDENTIFIER),
            is(parameter_list).set("parameters"),
            maybe(token(":")).then([is(type_).set("return type")]),
            is(code_body)
                .set("code body")
                .hint("A function must contain a code body"),
        ])
        .variables([
            IDENTIFIER_VAR,
            node_var("parameters"),
            node_var("return type"),
            node_var("code body"),
        ])
        .build();

    let systems = parser
        .grammar
        .new_node("systems")
        .rules([
            is(keyword("systems")).commit(),
            is(token("{")),
            while_(ident_path).set("systems"),
            is(token("}")),
        ])
        .variables([list_var("systems")])
        .build();

    let resources = parser
        .grammar
        .new_node("resources")
        .rules([
            is(keyword("resources")).commit(),
            is(token("{")),
            loop_().then([is_one_of([
                option(value).set("resources"),
                option(token("}")).return_node(),
            ])
            .hint("A list of simple values")]),
        ])
        .variables([list_var("resources")])
        .build();

    let scheduler = parser
        .grammar
        .new_node("scheduler")
        .has(docstr, "docs")
        .rules([
            is(keyword("scheduler")).commit().start(),
            is(ident).set(IDENTIFIER),
            is(token("{")),
            maybe(resources).set("resources"),
            maybe(systems).set("systems"),
            is(token("}"))
                .hint("Scheduler must at most contain resources and systems in this order"),
        ])
        .variables([IDENTIFIER_VAR, node_var("resources"), node_var("systems")])
        .build();

    let system = parser
        .grammar
        .new_node("system")
        .has(docstr, "docs")
        .rules([
            is(keyword("system")).commit().start(),
            is(ident).set(IDENTIFIER),
            is(code_body)
                .set("code body")
                .hint("A system must contain a code body"),
        ])
        .variables([IDENTIFIER_VAR, node_var("code body")])
        .build();

    let tls = parser
        .grammar
        .new_enum("top level statement")
        .options([scheduler, function, system])
        .build();

    parser
        .grammar
        .new_node("entry")
        .has(tl_docstr, "docs")
        .rules([loop_().then([is_one_of([
            option(tls).set("top level statements"),
            option(eof()).return_node(),
        ])])])
        .variables([list_var("top level statements")])
        .build();
    parser.parser.entry = Some("entry");

    let valid_result = Validator::default().validate(&parser);
    valid_result.print_all().unwrap();
    assert!(valid_result.pass());

    parser
}
